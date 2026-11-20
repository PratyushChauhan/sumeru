//! `sumeru servers …` / `sumeru keys …`: manage MCPs and client keys from scripts and agents.
//! Requests go to the running engine over `engine.sock` in the config dir, so the app stays the
//! only writer of `servers.json`. With no engine running, the CLI takes the store lock (the one
//! engines take at startup) and runs the same `api::dispatch` commands itself.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::{self, ApiKey};
use crate::gateway::AppInner;
use crate::{api, oauth};

const USAGE: &str = "usage:
  sumeru servers list
  sumeru servers add --name NAME --url URL [--header K=V]... [--bearer-stdin] [--disabled]
  sumeru servers add --name NAME --command PATH [--arg A]... [--env K=V]... [--disabled]
  sumeru servers remove|enable|disable SERVER
  sumeru servers login SERVER [--client-id ID] [--client-secret-stdin] [--no-browser]
  sumeru keys list [KEY]
  sumeru keys add NAME
  sumeru keys grant|revoke KEY SERVER
  sumeru keys token KEY
SERVER and KEY are a name (any case) or id. A --header/--env value of - is read from stdin.
Every command takes --json.";

const FLAGS: [&str; 6] = [
    "json",
    "bearer-stdin",
    "client-secret-stdin",
    "disabled",
    "help",
    "no-browser",
];
const OPTIONS: [&str; 7] = [
    "name",
    "url",
    "header",
    "command",
    "arg",
    "env",
    "client-id",
];

/// Parsed command line: positional words, `--option value` pairs, and bare `--flag`s.
#[derive(Default, Debug)]
struct Args {
    pos: Vec<String>,
    opts: Vec<(String, String)>,
    flags: Vec<String>,
}

impl Args {
    /// Inputs: args after `sumeru`. Outputs: parsed args, or an error on an unknown option or a
    /// missing value (`--name x` and `--name=x` both work).
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut a = Self::default();
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let Some(name) = arg.strip_prefix("--") else {
                a.pos.push(arg.clone());
                continue;
            };
            let (name, inline) = name
                .split_once('=')
                .map_or((name, None), |(n, v)| (n, Some(v)));
            if FLAGS.contains(&name) && inline.is_none() {
                a.flags.push(name.into());
            } else if OPTIONS.contains(&name) {
                // A missing value must not swallow the next flag (`--name --json`); --arg is raw.
                let value = inline.or_else(|| {
                    let next = it.next().map(String::as_str);
                    next.filter(|v| name == "arg" || !v.starts_with("--"))
                });
                a.opts.push((
                    name.into(),
                    value.ok_or(format!("--{name} needs a value"))?.into(),
                ));
            } else {
                return Err(format!("unknown option {arg}\n{USAGE}"));
            }
        }
        Ok(a)
    }

    /// Inputs: option name. Outputs: its last value, if given.
    fn opt(&self, name: &str) -> Option<&str> {
        self.opts
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Inputs: option name. Outputs: every value given for it, in order.
    fn all(&self, name: &str) -> Vec<&str> {
        self.opts
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Inputs: flag name. Outputs: true when it was passed.
    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }
}

/// Connection to whichever process owns the store.
enum Client {
    /// The running engine's `engine.sock`.
    #[cfg(unix)]
    Socket(BufReader<std::os::unix::net::UnixStream>),
    /// No engine running: this process holds the store lock and dispatches commands itself.
    Direct(Arc<AppInner>, tokio::runtime::Runtime, std::fs::File),
}

impl Client {
    /// Inputs: config dir. Outputs: a socket client when an engine answers, else a direct client
    /// once the store lock is free (retries ~5 s while an engine starts up), else an error.
    fn open(dir: &Path) -> Result<Self, String> {
        for _ in 0..50 {
            #[cfg(unix)]
            if let Ok(stream) = std::os::unix::net::UnixStream::connect(dir.join(api::SOCKET)) {
                return Ok(Self::Socket(BufReader::new(stream)));
            }
            if let Some(lock) = config::lock_store(dir, Duration::ZERO) {
                let app = AppInner::new(dir.to_path_buf()).map_err(|e| e.to_string())?;
                let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                return Ok(Self::Direct(Arc::new(app), rt, lock));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err("config is locked by another sumeru process (the app starting, or another CLI command); \
            retry, or restart Sumeru if it is running"
            .into())
    }

    /// Inputs: `api::dispatch` method and params. Outputs: the command's JSON result or error.
    /// A sign-in URL raised on the way is printed to stderr. Direct `list_keys` loads key tokens
    /// from the keychain only when asked (`{"tokens": true}`).
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match self {
            #[cfg(unix)]
            Self::Socket(conn) => {
                let req = json!({ "id": 1, "method": method, "params": params }).to_string();
                conn.get_mut()
                    .write_all(format!("{req}\n").as_bytes())
                    .map_err(|e| e.to_string())?;
                let mut line = String::new();
                loop {
                    line.clear();
                    if conn.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
                        return Err("Sumeru closed the connection".into());
                    }
                    let mut reply: Value =
                        serde_json::from_str(&line).map_err(|e| e.to_string())?;
                    if let Some(url) = reply["sign_in_url"].as_str() {
                        sign_in_hint(url);
                    } else if let Some(err) = reply["error"].as_str() {
                        return Err(err.into());
                    } else {
                        return Ok(reply["result"].take());
                    }
                }
            }
            Self::Direct(app, rt, _lock) => rt.block_on(async {
                if method == "list_keys" && params["tokens"] == true {
                    app.reload_tokens().await?;
                }
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                tokio::spawn(async move {
                    while let Some(url) = rx.recv().await {
                        sign_in_hint(&url);
                    }
                });
                oauth::SIGN_IN_URL
                    .scope(tx, api::dispatch(app, method, params))
                    .await
            }),
        }
    }
}

/// Inputs: OAuth authorize URL. Outputs: prints it to stderr (stdout stays the command's output).
fn sign_in_hint(url: &str) {
    eprintln!("To sign in, open (waiting up to 5 minutes):\n  {url}");
}

/// Inputs: args after `sumeru`. Outputs: exit code (0 ok, 1 error) after printing the result to
/// stdout (`--json`: one JSON value; errors as `{"error": …}`) or the error to stderr.
pub fn run(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let (code, line) = report(
        crate::sumeru_config_dir().and_then(|d| run_in(&d, args)),
        json,
    );
    if code == 0 || json {
        println!("{line}");
    } else {
        eprintln!("{line}");
    }
    code
}

/// Inputs: command result, whether `--json` was passed. Outputs: exit code and the line to print
/// (stdout on success or with `--json`, else stderr).
fn report(result: Result<(Value, String), String>, json: bool) -> (i32, String) {
    match result {
        Ok((value, _)) if json => (0, value.to_string()),
        Ok((_, text)) => (0, text),
        Err(err) if json => (1, json!({ "error": err }).to_string()),
        Err(err) => (1, format!("sumeru: {err}")),
    }
}

/// Inputs: config dir, args after `sumeru`. Outputs: (JSON result, human text) or an error. The
/// store is opened on the first command call, so usage errors never touch it.
fn run_in(dir: &Path, args: &[String]) -> Result<(Value, String), String> {
    let a = Args::parse(args)?;
    if a.flag("help") {
        return Ok((json!({ "usage": USAGE }), USAGE.into()));
    }
    let mut client = None;
    exec(&a, &mut |method, params| {
        if client.is_none() {
            client = Some(Client::open(dir)?);
        }
        client.as_mut().expect("opened").call(method, params)
    })
}

type Call<'a> = dyn FnMut(&str, Value) -> Result<Value, String> + 'a;

/// Inputs: parsed args, command caller. Outputs: (JSON result, human text) for the command.
fn exec(a: &Args, call: &mut Call) -> Result<(Value, String), String> {
    let pos: Vec<&str> = a.pos.iter().map(String::as_str).collect();
    match pos.as_slice() {
        ["servers", "list"] => {
            let servers = call("list_servers", Value::Null)?;
            let text = rows(&servers, |s| {
                let t = &s["transport"];
                let target = t["url"]
                    .as_str()
                    .or(t["command"].as_str())
                    .unwrap_or_default();
                let state = if s["enabled"] == true {
                    "enabled"
                } else {
                    "disabled"
                };
                format!(
                    "{}\t{state}\t{target}\t{}",
                    text(&s["name"]),
                    text(&s["id"])
                )
            });
            Ok((servers, text))
        }
        ["servers", "add"] => add_server(a, call),
        ["servers", verb @ ("remove" | "enable" | "disable" | "login"), server] => {
            let servers = call("list_servers", Value::Null)?;
            let s = find(&servers, server, "server")?;
            let (id, name) = (s["id"].clone(), text(&s["name"]));
            let done = match *verb {
                "remove" => call("remove_server", json!({ "id": id })).map(|_| "removed"),
                "login" => {
                    let url = s["transport"]["url"]
                        .as_str()
                        .ok_or("only URL servers sign in")?;
                    let secret = a.flag("client-secret-stdin").then(read_stdin).transpose()?;
                    let params = json!({ "url": url, "id": id, "clientId": a.opt("client-id"),
                        "clientSecret": secret, "noBrowser": a.flag("no-browser") });
                    call("start_mcp_oauth", params).map(|_| "signed in to")
                }
                _ => {
                    let enabled = *verb == "enable";
                    call(
                        "set_server_enabled",
                        json!({ "id": id, "enabled": enabled }),
                    )
                    .map(|_| if enabled { "enabled" } else { "disabled" })
                }
            }?;
            Ok((json!({ "id": id, "name": name }), format!("{done} {name}")))
        }
        ["keys", "list"] | ["keys", "list", _] => {
            let (keys, servers) = (
                call("list_keys", Value::Null)?,
                call("list_servers", Value::Null)?,
            );
            let only = pos
                .get(2)
                .map(|k| find(&keys, k, "key"))
                .transpose()?
                .map(|k| k["id"].clone());
            let views: Vec<Value> = list(&keys)
                .iter()
                .filter(|k| only.as_ref().is_none_or(|id| k["id"] == *id))
                .map(|k| key_view(k, &servers))
                .collect::<Result<_, _>>()?;
            let views = Value::from(views);
            let text = rows(&views, |k| {
                let names: Vec<_> = list(&k["servers"])
                    .iter()
                    .map(|s| text(&s["name"]))
                    .collect();
                format!(
                    "{}\t{}\t{}\t{}",
                    text(&k["name"]),
                    text(&k["mode"]),
                    names.join(", "),
                    text(&k["id"])
                )
            });
            Ok((views, text))
        }
        ["keys", "add", name] => {
            let id = call(
                "upsert_key",
                json!({ "name": name, "mode": "include", "mcpIds": [] }),
            )?;
            Ok((
                json!({ "id": id, "name": name }),
                format!("added key {name} ({})", text(&id)),
            ))
        }
        ["keys", verb @ ("grant" | "revoke"), key, server] => {
            let (keys, servers) = (
                call("list_keys", Value::Null)?,
                call("list_servers", Value::Null)?,
            );
            let mut k: ApiKey = serde_json::from_value(find(&keys, key, "key")?.clone())
                .map_err(|e| e.to_string())?;
            let s = find(&servers, server, "server")?;
            k.set_access(s["id"].as_str().unwrap_or_default(), *verb == "grant");
            call(
                "set_key_access",
                json!({ "keyId": k.id, "mcpId": s["id"], "allow": *verb == "grant" }),
            )?;
            let view = key_view(
                &serde_json::to_value(&k).map_err(|e| e.to_string())?,
                &servers,
            )?;
            let done = if *verb == "grant" {
                "granted"
            } else {
                "revoked"
            };
            Ok((view, format!("{done} {} for {}", text(&s["name"]), k.name)))
        }
        ["keys", "token", key] => {
            let keys = call("list_keys", json!({ "tokens": true }))?;
            let k = find(&keys, key, "key")?;
            let token = text(&k["token"]);
            if token.is_empty() {
                return Err(
                    "key token not loaded yet (is Sumeru waiting on a keychain prompt?)".into(),
                );
            }
            Ok((
                json!({ "id": k["id"], "name": k["name"], "token": token }),
                token,
            ))
        }
        _ => Err(USAGE.into()),
    }
}

/// Inputs: args (`--name` plus `--url`/`--header`/`--bearer-stdin` or `--command`/`--arg`/`--env`),
/// command caller. Outputs: the new server's id and name; secrets go to the keychain, not the file.
fn add_server(a: &Args, call: &mut Call) -> Result<(Value, String), String> {
    let name = a.opt("name").ok_or("--name is required")?;
    let http_only = !a.all("header").is_empty() || a.flag("bearer-stdin");
    let stdio_only = !a.all("env").is_empty() || !a.all("arg").is_empty();
    let (transport, secrets) = match (a.opt("url"), a.opt("command")) {
        (Some(url), None) if !stdio_only => {
            let headers = pairs(a.all("header"), "--header")?;
            let bearer = a.flag("bearer-stdin").then(read_stdin).transpose()?;
            let transport = json!({ "kind": "http", "url": url, "header_keys": headers.keys().collect::<Vec<_>>(),
                "has_bearer": bearer.is_some() });
            (transport, json!({ "headers": headers, "bearer": bearer }))
        }
        (None, Some(command)) if !http_only => {
            let env = pairs(a.all("env"), "--env")?;
            let transport = json!({ "kind": "stdio", "command": command, "args": a.all("arg"),
                "env_keys": env.keys().collect::<Vec<_>>() });
            (transport, json!({ "env": env }))
        }
        _ => {
            return Err(
                "pass --url (with --header/--bearer-stdin) or --command (with --arg/--env)".into(),
            )
        }
    };
    let params = json!({ "name": name, "enabled": !a.flag("disabled"), "transport": transport,
        "secrets": secrets });
    let id = call("upsert_server", params)?;
    Ok((
        json!({ "id": id, "name": name }),
        format!("added {name} ({})", text(&id)),
    ))
}

/// Inputs: `K=V` strings (`K=-` reads V from stdin), option name for errors. Outputs: map of
/// names to values.
fn pairs(raw: Vec<&str>, option: &str) -> Result<BTreeMap<String, String>, String> {
    raw.into_iter()
        .map(|kv| match kv.split_once('=') {
            Some((k, "-")) if !k.trim().is_empty() => Ok((k.trim().into(), read_stdin()?)),
            Some((k, v)) if !k.trim().is_empty() && !v.is_empty() => {
                Ok((k.trim().into(), v.into()))
            }
            _ => Err(format!("{option} expects NAME=VALUE")),
        })
        .collect()
}

/// Inputs: none. Outputs: stdin with surrounding whitespace trimmed (for secrets kept off argv);
/// an error on a second call, since stdin holds one value per command.
fn read_stdin() -> Result<String, String> {
    static USED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if USED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("only one value per command can come from stdin".into());
    }
    let mut s = String::new();
    std::io::stdin()
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    Some(s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("stdin was empty".into())
}

/// Inputs: key JSON (from `list_keys`), servers JSON. Outputs: `{id, name, mode, servers}` with
/// the servers this key can reach; never the token.
fn key_view(key: &Value, servers: &Value) -> Result<Value, String> {
    let k: ApiKey = serde_json::from_value(key.clone()).map_err(|e| e.to_string())?;
    let reach: Vec<Value> = list(servers)
        .iter()
        .filter(|s| s["id"].as_str().is_some_and(|id| k.allows(id)))
        .map(|s| json!({ "id": s["id"], "name": s["name"] }))
        .collect();
    Ok(json!({ "id": k.id, "name": k.name, "mode": k.mode, "servers": reach }))
}

/// Inputs: JSON array of `{id, name}` objects, query, kind for errors. Outputs: the entry whose id
/// matches exactly, else whose name matches ignoring case and surrounding spaces.
fn find<'a>(items: &'a Value, query: &str, kind: &str) -> Result<&'a Value, String> {
    let q = query.trim().to_lowercase();
    let items = list(items);
    items
        .iter()
        .find(|i| i["id"] == query)
        .or_else(|| {
            items
                .iter()
                .find(|i| text(&i["name"]).trim().to_lowercase() == q)
        })
        .ok_or_else(|| format!("no {kind} named {query:?}"))
}

/// Inputs: JSON value. Outputs: its array items (empty when not an array).
fn list(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

/// Inputs: JSON value. Outputs: its string content (empty when not a string).
fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().into()
}

/// Inputs: JSON array, row formatter. Outputs: one line per item, or "(none)".
fn rows(items: &Value, row: impl Fn(&Value) -> String) -> String {
    let lines: Vec<String> = list(items).iter().map(row).collect();
    if lines.is_empty() {
        "(none)".into()
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inputs: config dir, command words. Outputs: the command's JSON result or error.
    fn sumeru(dir: &Path, words: &str) -> Result<Value, String> {
        let args: Vec<String> = words.split(' ').map(String::from).collect();
        run_in(dir, &args).map(|(v, _)| v)
    }

    #[test]
    fn reports_exit_codes_and_json_errors() {
        let ok = || Ok((json!({ "id": "x" }), "added x".to_string()));
        assert_eq!(report(ok(), false), (0, "added x".into()));
        assert_eq!(report(ok(), true), (0, r#"{"id":"x"}"#.into()));
        assert_eq!(
            report(Err("no key".into()), false),
            (1, "sumeru: no key".into())
        );
        assert_eq!(
            report(Err("no key".into()), true),
            (1, r#"{"error":"no key"}"#.into())
        );
    }

    #[test]
    fn parses_options_flags_and_rejects_unknown() {
        let args = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        let a = Args::parse(&args(
            "servers add --name=A --url u --header X=1 --header Y=2 --json",
        ))
        .unwrap();
        assert_eq!(a.pos, ["servers", "add"]);
        assert_eq!((a.opt("name"), a.opt("url")), (Some("A"), Some("u")));
        assert_eq!(a.all("header"), ["X=1", "Y=2"]);
        assert!(a.flag("json") && !a.flag("disabled"));
        assert!(Args::parse(&args("servers add --name"))
            .unwrap_err()
            .contains("needs a value"));
        assert!(Args::parse(&args("keys list --nope"))
            .unwrap_err()
            .contains("unknown option"));
        assert!(pairs(vec!["novalue"], "--header").is_err());
        let dir = tempfile::tempdir().unwrap();
        assert!(sumeru(dir.path(), "keys frob")
            .unwrap_err()
            .starts_with("usage"));
        assert!(
            !dir.path().join("engine.lock").exists(),
            "usage errors must not open the store"
        );
    }

    #[test]
    fn server_verbs_make_one_atomic_call_and_bad_adds_fail() {
        let server = json!({ "id": "g", "name": "G", "enabled": false,
            "transport": { "kind": "http", "url": "https://g.example/mcp", "has_bearer": false } });
        let mut calls = vec![];
        let args = |s: &str| Args::parse(&s.split(' ').map(String::from).collect::<Vec<_>>());
        for (cmd, method, said) in [
            (
                "servers login g --no-browser",
                "start_mcp_oauth",
                "signed in to G",
            ),
            ("servers disable G", "set_server_enabled", "disabled G"),
        ] {
            let mut record = |m: &str, p: Value| {
                calls.push((m.to_string(), p));
                Ok(json!([server]))
            };
            let (out, text) = exec(&args(cmd).unwrap(), &mut record).unwrap();
            assert_eq!((out["id"].as_str(), text.as_str()), (Some("g"), said));
            let methods: Vec<_> = calls.drain(..).map(|(m, _)| m).collect();
            assert_eq!(
                methods,
                ["list_servers", method],
                "one atomic call, no stale re-save"
            );
        }
        let unused =
            &mut |_: &str, _: Value| -> Result<Value, String> { panic!("no call expected") };
        for bad in [
            "servers add --name A --url https://a.example --env K=1",
            "servers add --name A --command /bin/x --header K=1",
            "servers add --name A --url u --header K=",
        ] {
            assert!(exec(&args(bad).unwrap(), unused).is_err(), "{bad}");
        }
        assert!(args("servers add --name --json")
            .unwrap_err()
            .contains("needs a value"));
        assert_eq!(
            args("servers add --arg --root").unwrap().all("arg"),
            ["--root"]
        );
    }

    /// Inputs: config dir (engine socket or direct store). Outputs: asserts the agent flow: add a
    /// server, add a key, grant it, list the key's servers, read the token, disable, revoke, remove.
    fn agent_flow(dir: &Path) {
        let added = sumeru(
            dir,
            "servers add --name Granola --url https://mcp.granola.ai/mcp --header X-Team=t1 --json",
        )
        .unwrap();
        let id = text(&added["id"]);
        assert_eq!(
            config::get_header_secret(&id, "X-Team").as_deref(),
            Some("t1")
        );
        let file = std::fs::read_to_string(dir.join("servers.json")).unwrap();
        assert!(
            file.contains("X-Team") && !file.contains("t1"),
            "secret values stay off disk"
        );
        assert!(sumeru(
            dir,
            "servers add --name granola --url https://x.example/mcp"
        )
        .is_err());

        sumeru(dir, "keys add doppl").unwrap();
        assert_eq!(
            sumeru(dir, "keys list doppl").unwrap()[0]["servers"],
            json!([])
        );
        let granted = sumeru(dir, "keys grant DOPPL granola").unwrap();
        assert_eq!(granted["servers"], json!([{ "id": id, "name": "Granola" }]));
        let listed = sumeru(dir, "keys list doppl").unwrap();
        assert_eq!(listed[0]["servers"][0]["name"], "Granola");
        assert!(
            listed[0].get("token").is_none(),
            "keys list never shows tokens"
        );
        let token = sumeru(dir, "keys token doppl").unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(token.len(), 64);

        sumeru(dir, "servers disable Granola").unwrap();
        assert_eq!(sumeru(dir, "servers list").unwrap()[0]["enabled"], false);
        sumeru(dir, "servers enable Granola").unwrap();
        assert_eq!(
            config::get_header_secret(&id, "X-Team").as_deref(),
            Some("t1")
        );
        sumeru(dir, "keys revoke doppl Granola").unwrap();
        assert_eq!(
            sumeru(dir, "keys list doppl").unwrap()[0]["servers"],
            json!([])
        );
        sumeru(dir, "keys grant doppl Granola").unwrap();
        sumeru(dir, "servers remove granola").unwrap();
        assert_eq!(sumeru(dir, "servers list").unwrap(), json!([]));
        assert_eq!(
            sumeru(dir, "keys list doppl").unwrap()[0]["servers"],
            json!([])
        );
        assert!(sumeru(dir, "keys token nobody")
            .unwrap_err()
            .contains("no key named"));
        assert!(sumeru(dir, "servers login nothing")
            .unwrap_err()
            .contains("no server named"));
    }

    /// Inputs: none. Outputs: temp config dir, runtime, and an engine serving its socket there.
    #[cfg(unix)]
    fn engine() -> (tempfile::TempDir, tokio::runtime::Runtime, Arc<AppInner>) {
        config::use_memory_secrets();
        let dir = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let app = Arc::new(AppInner::new(dir.path().into()).unwrap());
        std::mem::forget(config::lock_store(dir.path(), Duration::ZERO).unwrap());
        rt.spawn(api::serve_socket(app.clone()));
        for _ in 0..500 {
            if dir.path().join(api::SOCKET).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        (dir, rt, app)
    }

    #[cfg(unix)]
    #[test]
    fn manages_servers_and_keys_through_the_engine_socket() {
        let (dir, rt, app) = engine();
        rt.block_on(app.reload_tokens()).unwrap();
        // The engine owns the lock, so every call below must have gone through its socket.
        assert!(config::lock_store(dir.path(), Duration::ZERO).is_none());
        agent_flow(dir.path());
        let in_engine = rt
            .block_on(api::dispatch(&app, "list_keys", Value::Null))
            .unwrap();
        assert!(
            list(&in_engine).iter().any(|k| k["name"] == "doppl"),
            "engine memory is current"
        );
    }

    #[cfg(unix)]
    #[test]
    fn parallel_cli_edits_are_all_kept() {
        let (dir, _rt, _app) = engine();
        let d = dir.path();
        let each = |cmd: &(dyn Fn(usize) -> String + Sync)| {
            std::thread::scope(|t| {
                (0..20).for_each(|i| drop(t.spawn(move || sumeru(d, &cmd(i)).unwrap())))
            });
        };
        each(&|i| format!("servers add --name s{i} --url https://s{i}.example/mcp"));
        sumeru(d, "keys add k").unwrap();
        each(&|i| format!("keys grant k s{i}"));
        assert_eq!(list(&sumeru(d, "servers list").unwrap()).len(), 20);
        assert_eq!(
            list(&sumeru(d, "keys list k").unwrap()[0]["servers"]).len(),
            20
        );
    }

    /// Inputs: none. Outputs: base URL of a local fake OAuth provider (metadata, DCR, tokens).
    async fn fake_provider() -> String {
        use axum::{
            routing::{get, post},
            Json,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let resource = json!({ "authorization_servers": [base] });
        let meta = json!({ "authorization_endpoint": format!("{base}/authorize"),
            "token_endpoint": format!("{base}/token"), "registration_endpoint": format!("{base}/register"),
            "code_challenge_methods_supported": ["S256"] });
        let router = axum::Router::new()
            .route(
                "/.well-known/oauth-protected-resource/mcp",
                get(|| async { Json(resource) }),
            )
            .route(
                "/.well-known/oauth-authorization-server",
                get(|| async { Json(meta) }),
            )
            .route(
                "/register",
                post(|| async { Json(json!({ "client_id": "cid" })) }),
            )
            .route(
                "/token",
                post(|| async { Json(json!({ "access_token": "at", "refresh_token": "rt" })) }),
            );
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        base
    }

    #[cfg(unix)]
    #[test]
    fn login_streams_the_sign_in_url_then_stores_tokens() {
        let (dir, rt, _app) = engine();
        let base = rt.block_on(fake_provider());
        let added = sumeru(
            dir.path(),
            &format!("servers add --name Fake --url {base}/mcp"),
        );
        let id = text(&added.unwrap()["id"]);
        // Inputs: none. Outputs: a socket that asked to sign in, and its first reply line.
        let start = || {
            let sock = dir.path().join(api::SOCKET);
            let mut conn = BufReader::new(std::os::unix::net::UnixStream::connect(sock).unwrap());
            let params = json!({ "url": format!("{base}/mcp"), "id": id, "noBrowser": true });
            let req = json!({ "id": 1, "method": "start_mcp_oauth", "params": params });
            conn.get_mut()
                .write_all(format!("{req}\n").as_bytes())
                .unwrap();
            let line = next_line(&mut conn);
            (conn, line)
        };
        drop(start()); // Ctrl-C mid sign-in: the engine drops it, so a retry is not stuck
        let (mut conn, event) = start();
        let url = url::Url::parse(event["sign_in_url"].as_str().unwrap()).unwrap();
        assert!(
            url.as_str().starts_with(&format!("{base}/authorize?")),
            "{url}"
        );
        let state = url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned();
        let callback = format!("{}?code=c&state={state}", oauth::REDIRECT_URI);
        rt.block_on(reqwest::get(callback)).unwrap();
        assert_eq!(next_line(&mut conn), json!({ "id": 1, "result": null }));
        assert_eq!(config::get_bearer_secret(&id).as_deref(), Some("at"));
        assert_eq!(config::get_oauth_refresh(&id).as_deref(), Some("rt"));
        let listed = sumeru(dir.path(), "servers list").unwrap();
        assert_eq!(
            listed[0]["transport"]["has_bearer"], true,
            "gateway now sends the token"
        );
    }

    /// Inputs: socket reader. Outputs: its next line as JSON.
    #[cfg(unix)]
    fn next_line(conn: &mut BufReader<std::os::unix::net::UnixStream>) -> Value {
        let mut line = String::new();
        conn.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn edits_the_store_directly_when_no_engine_runs() {
        config::use_memory_secrets();
        let dir = tempfile::tempdir().unwrap();
        agent_flow(dir.path());
        assert!(
            config::lock_store(dir.path(), Duration::ZERO).is_some(),
            "lock released after use"
        );
    }
}
