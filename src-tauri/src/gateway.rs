//! Authenticated Streamable HTTP MCP gateway with three meta-tools.

use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    extract::Request,
    http::{header, HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Router,
};
use tokio_stream::StreamExt;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, Extensions, Implementation, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ServiceExt,
};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{self, redact, ApiKey, AppConfig, McpServer, BIND_ADDR, DEFAULT_KEY_ID, ENDPOINT_URL},
    pool::SharedPool,
};

#[derive(Clone)]
pub struct AppInner {
    pub dir: std::path::PathBuf,
    pub config: Arc<RwLock<AppConfig>>,
    pub pool: SharedPool,
    /// (key id, bearer token) for every API key; empty in stdio mode.
    pub tokens: Arc<RwLock<Vec<(String, String)>>>,
    pub running: Arc<AtomicBool>,
    pub exiting: Arc<AtomicBool>,
    lifecycle: Arc<Mutex<()>>,
    shutdown: Arc<RwLock<Option<CancellationToken>>>,
    server_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl AppInner {
    /// Inputs: config directory, whether to load key tokens from the keychain. Outputs: app state.
    fn load(dir: std::path::PathBuf, with_tokens: bool) -> Result<Self, config::ConfigError> {
        let cfg = config::load_config(&dir)?;
        let tokens = if with_tokens {
            config::key_tokens(&cfg)?
        } else {
            Vec::new()
        };
        Ok(Self {
            dir,
            config: Arc::new(RwLock::new(cfg)),
            pool: Arc::new(crate::pool::ClientPool::new()),
            tokens: Arc::new(RwLock::new(tokens)),
            running: Arc::new(AtomicBool::new(false)),
            exiting: Arc::new(AtomicBool::new(false)),
            lifecycle: Arc::new(Mutex::new(())),
            shutdown: Arc::new(RwLock::new(None)),
            server_task: Arc::new(RwLock::new(None)),
        })
    }

    /// Inputs: config directory. Outputs: app state with every key's keychain token.
    pub fn new(dir: std::path::PathBuf) -> Result<Self, config::ConfigError> {
        Self::load(dir, true)
    }

    /// Inputs: config directory. Outputs: app state for stdio (no bearer tokens).
    pub fn for_stdio(dir: std::path::PathBuf) -> Result<Self, config::ConfigError> {
        Self::load(dir, false)
    }

    /// Inputs: none. Outputs: the Default key's token (used for health probes).
    pub async fn default_token(&self) -> String {
        let tokens = self.tokens.read().await;
        tokens
            .iter()
            .find(|(id, _)| id == DEFAULT_KEY_ID)
            .map(|(_, t)| t.clone())
            .unwrap_or_default()
    }

    /// Inputs: none. Outputs: unit after re-reading every key's token (after key edits).
    pub async fn reload_tokens(&self) -> Result<(), String> {
        let tokens = config::key_tokens(&*self.config.read().await).map_err(|e| e.to_string())?;
        *self.tokens.write().await = tokens;
        Ok(())
    }

    /// Inputs: none. Outputs: server matching id, if present.
    pub async fn find(&self, id: &str) -> Option<McpServer> {
        self.config
            .read()
            .await
            .servers
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }
}

#[derive(Clone)]
struct Gateway {
    app: Arc<AppInner>,
    /// Key id for stdio sessions (`--key`); HTTP requests carry theirs as a `KeyId` extension.
    stdio_key: Option<String>,
}

/// API key id resolved from the request bearer by `auth_middleware`.
#[derive(Clone)]
struct KeyId(String);

impl Gateway {
    /// Inputs: request extensions. Outputs: the API key scoping this request, if it still exists.
    async fn key(&self, ext: &Extensions) -> Option<ApiKey> {
        let id = ext
            .get::<axum::http::request::Parts>()
            .and_then(|p| p.extensions.get::<KeyId>())
            .map(|k| k.0.clone())
            .or_else(|| self.stdio_key.clone())?;
        let cfg = self.app.config.read().await;
        cfg.keys.iter().find(|k| k.id == id).cloned()
    }

    /// Inputs: request extensions, mcp id. Outputs: server when it exists and the key may reach it.
    async fn scoped(&self, ext: &Extensions, mcp_id: &str) -> Option<McpServer> {
        let key = self.key(ext).await?;
        self.app.find(mcp_id).await.filter(|s| key.allows(&s.id))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct McpIdArgs {
    /// Stable MCP identifier.
    mcp_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExecuteArgs {
    mcp_id: String,
    tool_name: String,
    #[serde(default)]
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Serialize)]
struct McpInfo {
    id: String,
    name: String,
    transport: String,
    enabled: bool,
}

#[tool_router]
impl Gateway {
    /// Inputs: request extensions (API key). Outputs: summaries of MCPs the key may reach.
    #[tool(description = "List configured MCP servers available through Sumeru")]
    async fn list_mcps(&self, ext: Extensions) -> Result<CallToolResult, rmcp::ErrorData> {
        let key = self.key(&ext).await;
        let cfg = self.app.config.read().await;
        let infos: Vec<McpInfo> = cfg
            .servers
            .iter()
            .filter(|s| key.as_ref().is_some_and(|k| k.allows(&s.id)))
            .map(|s| McpInfo {
                id: s.id.clone(),
                name: s.name.clone(),
                transport: match s.transport {
                    config::McpTransport::Stdio { .. } => "stdio".into(),
                    config::McpTransport::Http { .. } => "http".into(),
                },
                enabled: s.enabled,
            })
            .collect();
        // Cursor validates structuredContent as a JSON object (record), not an array.
        let structured = serde_json::json!({ "mcps": infos });
        Ok(CallToolResult::structured(structured))
    }

    /// Inputs: mcp_id, request extensions (API key). Outputs: upstream tool names, descriptions,
    /// and schemas; MCPs outside the key's scope are reported as unknown.
    #[tool(description = "List tools exposed by a configured MCP server")]
    async fn list_mcp_tools(
        &self,
        Parameters(McpIdArgs { mcp_id }): Parameters<McpIdArgs>,
        ext: Extensions,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let Some(server) = self.scoped(&ext, &mcp_id).await else {
            return Ok(CallToolResult::structured_error(serde_json::json!({
                "error": "unknown mcp_id"
            })));
        };
        match self.app.pool.list_tools(&server).await {
            Ok(tools) => {
                // Cursor validates structuredContent as a JSON object (record), not an array.
                let structured = serde_json::json!({ "tools": tools });
                Ok(CallToolResult::structured(structured))
            }
            Err(e) => {
                Ok(CallToolResult::structured_error(serde_json::json!({
                    "error": redact(&e.to_string())
                })))
            }
        }
    }

    /// Inputs: mcp_id, tool_name, arguments, request extensions (API key).
    /// Outputs: upstream CallToolResult; MCPs outside the key's scope are reported as unknown.
    #[tool(description = "Execute a tool on a configured MCP server")]
    async fn execute_mcp_tool(
        &self,
        Parameters(ExecuteArgs {
            mcp_id,
            tool_name,
            arguments,
        }): Parameters<ExecuteArgs>,
        ext: Extensions,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let Some(server) = self.scoped(&ext, &mcp_id).await else {
            return Ok(CallToolResult::structured_error(serde_json::json!({
                "error": "unknown mcp_id"
            })));
        };
        match self
            .app
            .pool
            .call_tool(&server, &tool_name, arguments)
            .await
        {
            Ok(result) => Ok(cursor_safe_tool_result(result)),
            Err(e) => {
                Ok(CallToolResult::structured_error(serde_json::json!({
                    "error": redact(&e.to_string())
                })))
            }
        }
    }
}

/// Inputs: upstream CallToolResult. Outputs: same with object-only structuredContent.
fn cursor_safe_tool_result(mut result: CallToolResult) -> CallToolResult {
    if let Some(sc) = result.structured_content.take() {
        result.structured_content = Some(match sc {
            serde_json::Value::Object(_) => sc,
            other => serde_json::json!({ "result": other }),
        });
    }
    result
}

impl Gateway {
    /// Inputs: none. Outputs: gateway tool definitions after router schema build.
    fn list_gateway_tools() -> Vec<rmcp::model::Tool> {
        Self::tool_router().list_all()
    }
}

#[tool_handler(router = Self::tool_router())]
impl rmcp::ServerHandler for Gateway {
    /// Inputs: none. Outputs: server capabilities and implementation info.
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "sumeru",
                env!("CARGO_PKG_VERSION"),
            ))
    }

    /// Inputs: pagination params and request context. Outputs: gateway tool list.
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let tools = Self::list_gateway_tools();
        Ok(ListToolsResult {
            tools,
            meta: None,
            next_cursor: None,
        })
    }
}

/// Inputs: two strings. Outputs: true when equal in constant time for equal lengths.
fn ct_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in ab.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Inputs: request headers, (key id, token) pairs. Outputs: id of the key whose token matches.
fn key_for_bearer(headers: &HeaderMap, tokens: &[(String, String)]) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let presented = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    tokens
        .iter()
        .find(|(_, t)| ct_eq(presented, t))
        .map(|(id, _)| id.clone())
}

/// Inputs: app state and request. Outputs: 403/401, or the downstream response with the
/// matched API key attached as a `KeyId` extension for tool handlers.
async fn auth_middleware(
    axum::extract::State(app): axum::extract::State<Arc<AppInner>>,
    mut req: Request,
    next: Next,
) -> Response {
    if req.headers().contains_key(header::ORIGIN) {
        return (
            StatusCode::FORBIDDEN,
            "browser Origin requests are not allowed",
        )
            .into_response();
    }
    let key_id = key_for_bearer(req.headers(), &app.tokens.read().await);
    let Some(key_id) = key_id else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "unauthorized",
        )
            .into_response();
    };
    req.extensions_mut().insert(KeyId(key_id));
    next.run(req).await
}

/// Soft GET SSE for Cursor peers (rmcp stateless would return 405; POST stays JSON).
///
/// Inputs: request.
/// Outputs: keep-alive SSE for GET, else next middleware.
async fn soft_get_sse(req: Request, next: Next) -> Response {
    if req.method() != Method::GET {
        return next.run(req).await;
    }
    // pending() keeps the stream open so KeepAlive pings continue (avoids Cursor reconnect loops).
    let stream = tokio_stream::once(Ok::<_, Infallible>(
        Event::default().comment("connected"),
    ))
    .chain(tokio_stream::pending());
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("ping"),
        )
        .into_response()
}

/// Inputs: shared app state and shutdown token. Outputs: HTTP router for MCP traffic.
fn gateway_router(app: Arc<AppInner>, child: CancellationToken) -> Router {
    let gateway = Gateway {
        app: app.clone(),
        stdio_key: None,
    };
    let service = StreamableHttpService::new(
        {
            let gateway = gateway.clone();
            move || Ok(gateway.clone())
        },
        LocalSessionManager::default().into(),
        // Stateless JSON POSTs; soft_get_sse upgrades GET 405 → 200 keep-alive SSE.
        StreamableHttpServerConfig::default()
            .with_cancellation_token(child)
            .with_allowed_hosts(["127.0.0.1", "localhost", "127.0.0.1:7341", "localhost:7341"])
            .with_stateful_mode(false)
            .with_json_response(true),
    );

    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn(soft_get_sse))
        .layer(middleware::from_fn_with_state(app, auth_middleware))
}

/// Inputs: bearer token. Outputs: true when Sumeru on BIND_ADDR answers initialize.
pub async fn existing_endpoint_healthy(token: &str) -> bool {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build();
    let Ok(client) = client else {
        return false;
    };
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "sumeru-health", "version": "0" }
        }
    });
    let Ok(resp) = client
        .post(ENDPOINT_URL)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header(header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
    else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(v) = resp.json::<serde_json::Value>().await else {
        return false;
    };
    if v.get("error").is_some() {
        return false;
    }
    v.get("result")
        .and_then(|r| r.get("serverInfo"))
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        == Some("sumeru")
}

/// Inputs: shared app state. Outputs: Ok(()) when the funnel HTTP server is listening.
pub async fn start(app: Arc<AppInner>) -> Result<(), String> {
    let _guard = app.lifecycle.lock().await;
    if app.running.load(Ordering::SeqCst) {
        return Ok(());
    }
    let ct = CancellationToken::new();
    let router = gateway_router(app.clone(), ct.child_token());

    let listener = match TcpListener::bind(BIND_ADDR).await {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            // External process owns the port — do not mark local `running`.
            if existing_endpoint_healthy(&app.default_token().await).await {
                return Ok(());
            }
            return Err(format!(
                "{BIND_ADDR} already in use (quit the other sumeru, then Resume)"
            ));
        }
        Err(err) => return Err(err.to_string()),
    };
    let shutdown = ct.clone();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                shutdown.cancelled().await;
            })
            .await;
    });

    *app.shutdown.write().await = Some(ct);
    *app.server_task.write().await = Some(task);
    app.running.store(true, Ordering::SeqCst);
    Ok(())
}

/// Inputs: shared app state. Outputs: Ok(()) after stopping HTTP and clearing clients.
pub async fn stop(app: Arc<AppInner>) -> Result<(), String> {
    let _guard = app.lifecycle.lock().await;
    if let Some(ct) = app.shutdown.write().await.take() {
        ct.cancel();
    }
    if let Some(mut task) = app.server_task.write().await.take() {
        match tokio::time::timeout(std::time::Duration::from_secs(3), &mut task).await {
            Ok(_) => {}
            Err(_) => {
                task.abort();
                let _ = task.await;
            }
        }
    }
    app.pool.clear().await;
    app.running.store(false, Ordering::SeqCst);
    Ok(())
}

/// Inputs: none. Outputs: funnel public URL constant.
pub fn endpoint_url() -> &'static str {
    ENDPOINT_URL
}

/// Inputs: shared app state, API key id scoping the session. Outputs: Ok(()) after the session ends.
pub async fn serve_stdio(app: Arc<AppInner>, key_id: String) -> Result<(), String> {
    let gateway = Gateway {
        app,
        stdio_key: Some(key_id),
    };
    let running = gateway
        .serve(stdio())
        .await
        .map_err(|e| e.to_string())?;
    running.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    const TEST_TOKEN: &str = "test-token";
    const SCOPED_TOKEN: &str = "scoped-token";
    const PROTOCOL_VERSION: &str = "2025-06-18";

    #[test]
    fn bearer_matching_is_exact() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            "Bearer secret-token".parse().unwrap(),
        );
        let key = |t: &str| vec![("k".to_string(), t.to_string())];
        assert_eq!(key_for_bearer(&headers, &key("secret-token")), Some("k".into()));
        assert_eq!(key_for_bearer(&headers, &key("other")), None);
        assert_eq!(key_for_bearer(&HeaderMap::new(), &key("secret-token")), None);
    }

    /// Regression: Vec outputSchema is type "array" and panics tool_router at tools/list.
    #[test]
    fn tool_router_list_all_does_not_panic() {
        let tools = Gateway::list_gateway_tools();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&"list_mcps"));
        assert!(names.contains(&"list_mcp_tools"));
        assert!(names.contains(&"execute_mcp_tool"));
    }

    #[test]
    fn cursor_safe_wraps_array_structured_content() {
        let raw = CallToolResult::structured(serde_json::json!([1, 2]));
        let safe = cursor_safe_tool_result(raw);
        assert!(safe.structured_content.as_ref().unwrap().is_object());
        assert_eq!(
            safe.structured_content.unwrap()["result"],
            serde_json::json!([1, 2])
        );
        let obj = CallToolResult::structured(serde_json::json!({ "ok": true }));
        let kept = cursor_safe_tool_result(obj);
        assert_eq!(kept.structured_content.unwrap()["ok"], true);
    }

    /// Inputs: app config. Outputs: app state where TEST_TOKEN is the Default key and
    /// SCOPED_TOKEN is the "scoped" key (keys come from the config).
    fn test_app(config: AppConfig) -> Arc<AppInner> {
        let tokens = vec![
            (DEFAULT_KEY_ID.to_string(), TEST_TOKEN.to_string()),
            ("scoped".to_string(), SCOPED_TOKEN.to_string()),
        ];
        Arc::new(AppInner {
            dir: tempfile::tempdir().unwrap().keep(),
            config: Arc::new(RwLock::new(config)),
            pool: Arc::new(crate::pool::ClientPool::new()),
            tokens: Arc::new(RwLock::new(tokens)),
            running: Arc::new(AtomicBool::new(false)),
            exiting: Arc::new(AtomicBool::new(false)),
            lifecycle: Arc::new(Mutex::new(())),
            shutdown: Arc::new(RwLock::new(None)),
            server_task: Arc::new(RwLock::new(None)),
        })
    }

    /// Inputs: router, bearer token, session, request id, method, params.
    /// Outputs: status, headers, JSON body.
    async fn post_rpc(
        router: &Router,
        token: &str,
        session: Option<&str>,
        id: Option<u64>,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> (StatusCode, HeaderMap, serde_json::Value) {
        let mut body = serde_json::json!({ "jsonrpc": "2.0", "method": method });
        if let Some(id) = id {
            body["id"] = id.into();
        }
        if let Some(params) = params {
            body["params"] = params;
        }
        let mut builder = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, "127.0.0.1:7341")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header(header::CONTENT_TYPE, "application/json")
            .header("MCP-Protocol-Version", PROTOCOL_VERSION);
        if let Some(session) = session {
            builder = builder.header("Mcp-Session-Id", session);
        }
        let response = router
            .clone()
            .oneshot(builder.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = if body.is_empty() {
            serde_json::Value::Null
        } else {
            // Non-JSON bodies (e.g. 401 "unauthorized") come back as a string.
            serde_json::from_slice(&body)
                .unwrap_or_else(|_| String::from_utf8_lossy(&body).into_owned().into())
        };
        (status, headers, json)
    }

    /// Inputs: router and optional session. Outputs: HTTP status for GET /mcp.
    async fn get_mcp_status(router: &Router, session: Option<&str>) -> StatusCode {
        let mut builder = Request::builder()
            .method("GET")
            .uri("/mcp")
            .header(header::HOST, "127.0.0.1:7341")
            .header(header::AUTHORIZATION, format!("Bearer {TEST_TOKEN}"))
            .header(header::ACCEPT, "text/event-stream")
            .header("MCP-Protocol-Version", PROTOCOL_VERSION);
        if let Some(session) = session {
            builder = builder.header("Mcp-Session-Id", session);
        }
        router
            .clone()
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn tools_list_returns_gateway_tools_over_http() {
        let app = test_app(config::load_config(&tempfile::tempdir().unwrap().keep()).unwrap());
        let ct = CancellationToken::new();
        let router = gateway_router(app, ct.child_token());

        let initialize = post_rpc(
            &router,
            TEST_TOKEN,
            None,
            Some(1),
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "sumeru-test", "version": "0" }
            })),
        )
        .await;
        assert_eq!(initialize.0, StatusCode::OK);
        assert_eq!(
            initialize
                .1
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );

        let initialized =
            post_rpc(&router, TEST_TOKEN, None, None, "notifications/initialized", None).await;
        assert_eq!(initialized.0, StatusCode::ACCEPTED);

        // Soft GET returns 200 SSE keep-alive.
        assert_eq!(get_mcp_status(&router, None).await, StatusCode::OK);

        let started = std::time::Instant::now();
        let list = post_rpc(&router, TEST_TOKEN, None, Some(2), "tools/list", None).await;
        assert_eq!(list.0, StatusCode::OK);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(
            list.1
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        let names: Vec<_> = list.2["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["execute_mcp_tool", "list_mcp_tools", "list_mcps"]);

        let ping = post_rpc(&router, TEST_TOKEN, None, Some(3), "ping", None).await;
        assert_eq!(ping.0, StatusCode::OK);

        let call = post_rpc(
            &router,
            TEST_TOKEN,
            None,
            Some(4),
            "tools/call",
            Some(serde_json::json!({ "name": "list_mcps", "arguments": {} })),
        )
        .await;
        assert_eq!(call.0, StatusCode::OK);
        assert_ne!(call.2["result"]["isError"], true);

        ct.cancel();
    }

    /// Inputs: none. Outputs: config with stdio MCPs `a` and `b`, the Default key, and a
    /// "scoped" key that may only reach `a`.
    fn scoped_config() -> AppConfig {
        let server = |id: &str| McpServer {
            id: id.into(),
            name: id.into(),
            enabled: true,
            transport: config::McpTransport::Stdio {
                command: "/bin/true".into(),
                args: vec![],
                env_keys: vec![],
            },
        };
        let mut cfg = config::load_config(&tempfile::tempdir().unwrap().keep()).unwrap();
        cfg.servers = vec![server("a"), server("b")];
        cfg.keys.push(ApiKey {
            id: "scoped".into(),
            name: "Scoped".into(),
            mode: config::KeyMode::Include,
            mcp_ids: vec!["a".into()],
        });
        cfg
    }

    /// Inputs: router, token, tool name, arguments. Outputs: HTTP status and tool result JSON.
    async fn call_tool(
        router: &Router,
        token: &str,
        name: &str,
        args: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let params = serde_json::json!({ "name": name, "arguments": args });
        let (status, _, body) = post_rpc(router, token, None, Some(9), "tools/call", Some(params)).await;
        (status, body["result"]["structuredContent"].clone())
    }

    #[tokio::test]
    async fn scoped_key_filters_and_blocks() {
        let ct = CancellationToken::new();
        let router = gateway_router(test_app(scoped_config()), ct.child_token());
        let ids = |v: serde_json::Value| -> Vec<String> {
            v["mcps"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap().to_string()).collect()
        };

        let (_, all) = call_tool(&router, TEST_TOKEN, "list_mcps", serde_json::json!({})).await;
        assert_eq!(ids(all), ["a", "b"]);
        let (_, scoped) = call_tool(&router, SCOPED_TOKEN, "list_mcps", serde_json::json!({})).await;
        assert_eq!(ids(scoped), ["a"]);

        let blocked = serde_json::json!({ "mcp_id": "b", "tool_name": "x" });
        let (_, tools) = call_tool(&router, SCOPED_TOKEN, "list_mcp_tools", blocked.clone()).await;
        assert_eq!(tools["error"], "unknown mcp_id");
        let (_, exec) = call_tool(&router, SCOPED_TOKEN, "execute_mcp_tool", blocked).await;
        assert_eq!(exec["error"], "unknown mcp_id");

        let (status, _) = call_tool(&router, "wrong-token", "list_mcps", serde_json::json!({})).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        ct.cancel();
    }

    #[tokio::test]
    async fn stdio_key_scopes_without_http_extensions() {
        let gateway = Gateway {
            app: test_app(scoped_config()),
            stdio_key: Some("scoped".into()),
        };
        let ext = Extensions::new();
        assert!(gateway.scoped(&ext, "a").await.is_some());
        assert!(gateway.scoped(&ext, "b").await.is_none());
        let unscoped = Gateway { stdio_key: None, ..gateway };
        assert!(unscoped.scoped(&ext, "a").await.is_none());
    }
}
