//! Lazy, reusable upstream MCP client cache.

use std::{collections::HashMap, sync::Arc, time::Duration};

use http::{HeaderName, HeaderValue};
use rmcp::{
    model::{CallToolRequestParams, CallToolResult, Tool},
    service::{Peer, RunningService},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, ConfigureCommandExt,
        StreamableHttpClientTransport, TokioChildProcess,
    },
    RoleClient, ServiceExt,
};
use tokio::{process::Command, sync::Mutex};

use crate::config::{
    self, get_bearer_secret, get_env_secret, get_header_secret, redact, transport_fingerprint,
    ConfigError, McpServer, McpTransport,
};

struct Cached {
    peer: Peer<RoleClient>,
    running: RunningService<RoleClient, ()>,
    fingerprint: String,
}

type Slot = Arc<Mutex<Option<Cached>>>;

#[derive(Default)]
pub struct ClientPool {
    slots: Mutex<HashMap<String, Slot>>,
}

impl ClientPool {
    /// Inputs: none. Outputs: empty pool.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inputs: mcp id. Outputs: per-id mutex slot for the cached client.
    async fn slot(&self, id: &str) -> Slot {
        let mut slots = self.slots.lock().await;
        slots
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }

    /// Inputs: mcp id. Outputs: unit after dropping any cached client.
    pub async fn invalidate(&self, id: &str) {
        let slot = {
            let mut slots = self.slots.lock().await;
            slots.remove(id)
        };
        if let Some(slot) = slot {
            if let Some(mut cached) = slot.lock().await.take() {
                let _ = cached
                    .running
                    .close_with_timeout(Duration::from_secs(2))
                    .await;
            }
        }
    }

    /// Inputs: none. Outputs: unit after closing every cached client.
    pub async fn clear(&self) {
        let slots = {
            let mut guard = self.slots.lock().await;
            std::mem::take(&mut *guard)
        };
        for (_, slot) in slots {
            if let Some(mut cached) = slot.lock().await.take() {
                let _ = cached
                    .running
                    .close_with_timeout(Duration::from_secs(2))
                    .await;
            }
        }
    }

    /// Inputs: server definition. Outputs: cloned peer for the live upstream.
    pub async fn peer(&self, server: &McpServer) -> Result<Peer<RoleClient>, ConfigError> {
        if !server.enabled {
            return Err(ConfigError::msg("mcp is disabled"));
        }
        let fp = transport_fingerprint(server);
        let slot = self.slot(&server.id).await;
        let mut guard = slot.lock().await;
        if let Some(cached) = guard.as_ref() {
            if cached.fingerprint == fp && !cached.peer.is_transport_closed() {
                return Ok(cached.peer.clone());
            }
        }
        if let Some(mut old) = guard.take() {
            drop(guard);
            let _ = old.running.close_with_timeout(Duration::from_secs(2)).await;
            guard = slot.lock().await;
            if let Some(cached) = guard.as_ref() {
                if cached.fingerprint == fp && !cached.peer.is_transport_closed() {
                    return Ok(cached.peer.clone());
                }
            }
        }
        let running = match connect(server).await {
            Err(err) if is_auth_error(&err) && refresh_bearer(server).await? => {
                connect(server).await?
            }
            other => other?,
        };
        let peer = running.peer().clone();
        *guard = Some(Cached {
            peer: peer.clone(),
            running,
            fingerprint: fp,
        });
        Ok(peer)
    }

    /// Inputs: server. Outputs: upstream tool list (reconnects once on closed transport).
    pub async fn list_tools(&self, server: &McpServer) -> Result<Vec<Tool>, ConfigError> {
        match self.peer(server).await?.list_all_tools().await {
            Ok(tools) => Ok(tools),
            Err(err) => {
                self.invalidate(&server.id).await;
                self.peer(server)
                    .await?
                    .list_all_tools()
                    .await
                    .map_err(|e| ConfigError::msg(redact(&format!("{err}; retry: {e}"))))
            }
        }
    }

    /// Inputs: server, tool name, arguments. Outputs: upstream CallToolResult (no auto-retry).
    pub async fn call_tool(
        &self,
        server: &McpServer,
        tool_name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, ConfigError> {
        let peer = self.peer(server).await?;
        let mut params = CallToolRequestParams::new(tool_name.to_string());
        if let Some(args) = arguments {
            params = params.with_arguments(args);
        }
        match peer.call_tool(params.clone()).await {
            // The upstream rejected the request, so a retry after a reconnect is safe.
            Err(e) if is_unauthorized(&e.to_string()) => {
                self.invalidate(&server.id).await;
                self.peer(server).await?.call_tool(params).await
            }
            other => other,
        }
        .map_err(|e| ConfigError::msg(redact(&e.to_string())))
    }
}

/// Inputs: upstream error text. Outputs: true when it is an HTTP 401 (rmcp says "Auth required"
/// when the 401 carries `WWW-Authenticate`, else "HTTP 401").
fn is_unauthorized(err: &str) -> bool {
    err.contains("Auth required") || err.contains("HTTP 401")
}

/// Inputs: connect error. Outputs: true when the upstream rejected our credentials.
fn is_auth_error(err: &ConfigError) -> bool {
    is_unauthorized(&err.to_string())
}

/// Inputs: server. Outputs: Ok(true) when an OAuth refresh stored a new bearer; Ok(false) when
/// the server has no refresh token; Err asking to sign in again when the refresh is rejected.
async fn refresh_bearer(server: &McpServer) -> Result<bool, ConfigError> {
    let McpTransport::Http {
        url,
        has_bearer: true,
        ..
    } = &server.transport
    else {
        return Ok(false);
    };
    crate::oauth::refresh(url, &server.id).await.map_err(|e| {
        ConfigError::msg(format!(
            "{} session expired; sign in again ({e})",
            server.name
        ))
    })
}

/// Inputs: server definition. Outputs: connected RunningService.
async fn connect(server: &McpServer) -> Result<RunningService<RoleClient, ()>, ConfigError> {
    match &server.transport {
        McpTransport::Stdio {
            command,
            args,
            env_keys,
        } => {
            let cmd = Command::new(command).configure(|c| {
                c.args(args);
                c.env_clear();
                if let Ok(path) = std::env::var("PATH") {
                    c.env("PATH", path);
                }
                if let Ok(home) = std::env::var("HOME") {
                    c.env("HOME", home);
                }
                for key in env_keys {
                    if let Some(val) = get_env_secret(&server.id, key) {
                        c.env(key, val);
                    }
                }
            });
            let transport = TokioChildProcess::new(cmd)
                .map_err(|e| ConfigError::msg(redact(&e.to_string())))?;
            ().serve(transport)
                .await
                .map_err(|e| ConfigError::msg(redact(&e.to_string())))
        }
        McpTransport::Http {
            url,
            header_keys,
            has_bearer,
        } => {
            config::validate_http_url(url)?;
            let mut headers = HashMap::new();
            for key in header_keys {
                if let Some(val) = get_header_secret(&server.id, key) {
                    let name = HeaderName::from_bytes(key.as_bytes())
                        .map_err(|e| ConfigError::msg(e.to_string()))?;
                    let value =
                        HeaderValue::from_str(&val).map_err(|e| ConfigError::msg(e.to_string()))?;
                    headers.insert(name, value);
                }
            }
            let mut cfg = StreamableHttpClientTransportConfig::with_uri(url.clone())
                .custom_headers(headers)
                .reinit_on_expired_session(true);
            if *has_bearer {
                if let Some(token) = get_bearer_secret(&server.id) {
                    cfg = cfg.auth_header(token);
                }
            }
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(60))
                .build()
                .map_err(|e| ConfigError::msg(e.to_string()))?;
            let transport = StreamableHttpClientTransport::with_client(client, cfg);
            ().serve(transport)
                .await
                .map_err(|e| ConfigError::msg(redact(&e.to_string())))
        }
    }
}

/// Shared pool handle used by gateway and UI commands.
pub type SharedPool = Arc<ClientPool>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::{get, post},
        Json,
    };
    use serde_json::{json, Value};

    /// Inputs: none. Outputs: base URL of a fake OAuth MCP whose `/mcp` accepts only bearer
    /// `new`, and whose `/token` trades refresh token `rt` (client `cid`) for it.
    async fn fake_oauth_mcp() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let resource = json!({ "authorization_servers": [base] });
        let meta = json!({ "authorization_endpoint": format!("{base}/authorize"),
            "token_endpoint": format!("{base}/token") });
        let mcp = |headers: HeaderMap, Json(req): Json<Value>| async move {
            if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer new") {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            let result = match req["method"].as_str() {
                Some("initialize") => json!({ "protocolVersion": "2025-06-18",
                    "capabilities": { "tools": {} }, "serverInfo": { "name": "fake", "version": "0" } }),
                Some("tools/list") => {
                    json!({ "tools": [{ "name": "ping", "inputSchema": { "type": "object" } }] })
                }
                _ => return StatusCode::ACCEPTED.into_response(),
            };
            Json(json!({ "jsonrpc": "2.0", "id": req["id"], "result": result })).into_response()
        };
        let token = |body: String| async move {
            let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
                .into_owned()
                .collect();
            let ok = form["grant_type"] == "refresh_token"
                && form["refresh_token"] == "rt"
                && form["client_id"] == "cid";
            match ok {
                true => {
                    Json(json!({ "access_token": "new", "refresh_token": "rt2" })).into_response()
                }
                false => (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "invalid_grant" })),
                )
                    .into_response(),
            }
        };
        let router = axum::Router::new()
            .route("/mcp", post(mcp))
            .route("/token", post(token))
            .route(
                "/.well-known/oauth-protected-resource/mcp",
                get(|| async { Json(resource) }),
            )
            .route(
                "/.well-known/oauth-authorization-server",
                get(|| async { Json(meta) }),
            );
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        base
    }

    /// Inputs: base URL, mcp id, saved refresh token. Outputs: an OAuth server whose access
    /// token (`old`) has expired.
    fn expired(base: &str, id: &str, refresh: &str) -> McpServer {
        let bearer = config::SecretMap {
            bearer: Some("old".into()),
            ..Default::default()
        };
        config::store_secrets(id, &bearer).unwrap();
        config::store_oauth_client(id, "cid", None).unwrap();
        config::store_oauth_refresh(id, refresh).unwrap();
        let url = format!("{base}/mcp");
        let transport = McpTransport::Http {
            url,
            header_keys: vec![],
            has_bearer: true,
        };
        McpServer {
            id: id.into(),
            name: "Fake".into(),
            enabled: true,
            transport,
        }
    }

    #[tokio::test]
    async fn expired_oauth_token_is_refreshed_then_reconnects() {
        config::use_memory_secrets();
        let base = fake_oauth_mcp().await;
        let server = expired(&base, "refresh-ok", "rt");
        let tools = ClientPool::new().list_tools(&server).await.unwrap();
        assert_eq!(tools[0].name, "ping");
        assert_eq!(get_bearer_secret(&server.id).as_deref(), Some("new"));
        assert_eq!(
            config::get_oauth_refresh(&server.id).as_deref(),
            Some("rt2")
        ); // rotated

        let revoked = expired(&base, "refresh-bad", "revoked");
        let err = ClientPool::new()
            .list_tools(&revoked)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Fake session expired; sign in again"), "{err}");
    }
}
