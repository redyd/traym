//! Runs the agents. Claude: one `claude -p` per turn, prompt on stdin, stream-json out.
//! Codex: one `codex app-server` per turn (JSON-RPC; unlike `codex exec` it streams text deltas).
//! Custom providers (OpenAI-compatible APIs) run in-process, see openai.rs.
//! Everything is normalized into `agent://event`: session / delta / message / tool / error / done.

use crate::Core;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

pub const BRIEF: &str = "You are the assistant built into traym, a markdown note editor. \
The user's notes are .md files in a vault: read and change them only through the traym tools. \
Write clean, sober markdown. Answer briefly, in the user's language.";

/// GUI apps often get a bare PATH, so also look where npm/bun/pipx install CLIs.
fn find_cli(name: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let extra = [".local/bin", "bin", ".bun/bin", ".npm-global/bin", ".cargo/bin"].map(|d| PathBuf::from(&home).join(d));
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(extra)
        .chain(["/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from))
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

const fn pick(model: &Option<String>) -> Option<&String> {
    match model {
        Some(m) if !m.is_empty() && !matches!(m.as_bytes(), b"default") => Some(m),
        _ => None,
    }
}

fn claude_args(mcp: &str, token: &str, session: Option<&str>, model: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec![];
    let server = json!({ "type": "http", "url": mcp, "headers": { "Authorization": format!("Bearer {token}") } });
    let cfg = json!({ "mcpServers": { "traym": server } }).to_string();
    for s in ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--strict-mcp-config"] {
        a.push(s.into());
    }
    a.extend(["--mcp-config".into(), cfg, "--allowedTools".into(), "mcp__traym".into()]);
    a.extend(["--append-system-prompt".into(), BRIEF.into()]);
    if let Some(s) = session {
        a.extend(["--resume".into(), s.into()]);
    }
    if let Some(m) = model {
        a.extend(["--model".into(), m.into()]);
    }
    a
}

/// thread/start (or thread/resume) for a Codex turn: read-only sandbox, only the traym MCP tools, auto-approved.
fn codex_thread(mcp: &str, token: &str, cwd: &str, session: Option<&str>, model: Option<&str>) -> Value {
    let mut p = json!({
        "cwd": cwd,
        "sandbox": "read-only",
        "approvalPolicy": "never",
        "developerInstructions": BRIEF,
        "config": {
            "mcp_servers.traym.url": mcp,
            "mcp_servers.traym.http_headers": { "Authorization": format!("Bearer {token}") },
            "mcp_servers.traym.default_tools_approval_mode": "approve",
        },
    });
    if let Some(m) = model {
        p["model"] = json!(m);
    }
    let method = if let Some(s) = session {
        p["threadId"] = json!(s);
        "thread/resume"
    } else {
        "thread/start"
    };
    json!({ "jsonrpc": "2.0", "id": 2, "method": method, "params": p })
}

/// One CLI JSON line -> zero or more UI events.
fn read(line: &Value) -> Vec<Value> {
    let mut out = vec![];
    if let Some(s) = line["session_id"].as_str() {
        out.push(json!({ "type": "session", "id": s }));
    }
    let p = &line["params"];
    let item = &p["item"];
    match (line["type"].as_str().unwrap_or(""), line["method"].as_str().unwrap_or("")) {
        // claude
        ("stream_event", _) => {
            let e = &line["event"];
            if e["type"] == "content_block_start" && e["content_block"]["type"] == "tool_use" {
                let name = e["content_block"]["name"].as_str().unwrap_or("").trim_start_matches("mcp__traym__");
                out.push(json!({ "type": "tool", "name": name }));
            }
            if e["delta"]["type"] == "text_delta" {
                out.push(json!({ "type": "delta", "text": e["delta"]["text"] }));
            }
        }
        ("assistant", _) => {
            for b in line["message"]["content"].as_array().into_iter().flatten() {
                if b["type"] == "text" {
                    out.push(json!({ "type": "message", "text": b["text"] }));
                }
            }
        }
        ("result", _) if line["is_error"] == true => {
            out.push(json!({ "type": "error", "text": line["result"].as_str().unwrap_or("error") }));
        }
        // codex app-server
        (_, "item/agentMessage/delta") => out.push(json!({ "type": "delta", "text": p["delta"] })),
        (_, "item/completed") if item["type"] == "agentMessage" => out.push(json!({ "type": "message", "text": item["text"] })),
        (_, "item/started") if item["type"] == "mcpToolCall" => out.push(json!({ "type": "tool", "name": item["tool"] })),
        (_, "error") if p["willRetry"] != true => out.push(json!({ "type": "error", "text": p["error"]["message"] })),
        (_, "turn/completed") if p["turn"]["status"] == "failed" => {
            out.push(json!({ "type": "error", "text": p["turn"]["error"]["message"].as_str().unwrap_or("error") }))
        }
        _ => {}
    }
    out
}

/// Which CLIs are installed, so the UI can grey out the others.
pub fn available() -> Value {
    json!({ "claude": find_cli("claude").is_some(), "codex": find_cli("codex").is_some() })
}

/// The CLI with its own directory on PATH (it may spawn node etc. from there).
fn cli(agent: &str) -> Result<(&'static str, Command), String> {
    let name = if agent == "codex" { "codex" } else { "claude" };
    let exe = find_cli(name).ok_or_else(|| format!("`{name}` not found: install it or add it to PATH"))?;
    let path = std::env::join_paths(exe.parent().into_iter().map(PathBuf::from).chain(
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default(),
    ))
    .map_err(|e| e.to_string())?;
    let mut cmd = Command::new(&exe);
    cmd.env("PATH", path).kill_on_drop(true);
    Ok((name, cmd))
}

/// Ask the CLI itself which models it offers: Claude answers the stream-json `initialize`
/// control request, Codex answers `model/list` on its app-server. -> [{ id, name, default }]
pub async fn models(agent: String) -> Result<Vec<Value>, String> {
    let (name, mut cmd) = cli(&agent)?;
    let codex = name == "codex";
    let (argv, req): (&[&str], String) = if codex {
        let msgs = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "traym", "version": "0.1" } } }),
            json!({ "jsonrpc": "2.0", "method": "initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "model/list", "params": {} }),
        ];
        (&["app-server"], msgs.map(|m| m.to_string() + "\n").concat())
    } else {
        let a = &["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--setting-sources", ""];
        (a, json!({ "type": "control_request", "request_id": "models", "request": { "subtype": "initialize" } }).to_string() + "\n")
    };
    let mut child = cmd
        .args(argv)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().unwrap(); // kept open: both exit on EOF before answering
    stdin.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let found = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while let Ok(Some(l)) = lines.next_line().await {
            let v: Value = serde_json::from_str(&l).unwrap_or_default();
            if codex && v["id"] == 2 {
                return v["result"]["data"].as_array().cloned();
            }
            if !codex && v["type"] == "control_response" {
                return v["response"]["response"]["models"].as_array().cloned();
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| format!("`{name}` did not return its model list"))?;
    Ok(found.iter().filter(|m| m["hidden"] != true).map(|m| model(codex, m)).collect())
}

/// `light`: the cheap, quick model traym picks until the user chooses (Haiku; for Codex, whatever calls itself fast/mini).
fn model(codex: bool, m: &Value) -> Value {
    let words = format!("{} {} {}", m["model"], m["value"], m["description"]).to_lowercase();
    let light = if codex { ["fast", "mini", "light", "efficient"].iter().any(|w| words.contains(w)) } else { m["value"] == "haiku" };
    if codex {
        json!({ "id": m["model"], "name": m["displayName"], "default": m["isDefault"] == true, "light": light })
    } else {
        json!({ "id": m["value"], "name": m["displayName"], "default": m["value"] == "default", "light": light })
    }
}

pub async fn send(core: Arc<Core>, agent: String, prompt: String, session: Option<String>, model: Option<String>) -> Result<(), String> {
    let emit = |v: Value| {
        let _ = core.app.emit("agent://event", v);
    };
    let r = match agent.strip_prefix("custom:") {
        Some(id) => crate::openai::send(core.clone(), id.to_string(), prompt, session, model).await,
        None => run_cli(&core, &agent, prompt, session, model).await,
    };
    if let Err(e) = &r {
        emit(json!({ "type": "error", "text": e }));
    }
    emit(json!({ "type": "done" }));
    Ok(())
}

async fn run_cli(core: &Arc<Core>, agent: &str, prompt: String, session: Option<String>, model: Option<String>) -> Result<(), String> {
    let (name, mut cmd) = cli(agent)?;
    let codex = name == "codex";
    let vault = core.vault()?;
    let (mcp, token) = (&core.mcp_url, &core.session_token);
    let args = if codex { vec!["app-server".to_string()] } else { claude_args(mcp, token, session.as_deref(), pick(&model).map(|m| m.as_str())) };
    let mut child = cmd
        .args(args)
        .current_dir(&vault)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take();
    let w = stdin.as_mut().unwrap();
    if codex {
        let init = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "traym", "version": env!("CARGO_PKG_VERSION") } } }),
            json!({ "jsonrpc": "2.0", "method": "initialized" }),
            codex_thread(mcp, token, &vault.to_string_lossy(), session.as_deref(), pick(&model).map(|m| m.as_str())),
        ];
        w.write_all(init.map(|m| m.to_string() + "\n").concat().as_bytes()).await.map_err(|e| e.to_string())?;
    } else {
        w.write_all(prompt.as_bytes()).await.map_err(|e| e.to_string())?;
        stdin = None; // EOF: claude starts
    }
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    *core.agent.lock().await = Some(child);

    let emit = |v: Value| {
        let _ = core.app.emit("agent://event", v);
    };
    let err_task = tokio::spawn(async move {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s).await;
        s
    });
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(l)) = lines.next_line().await {
        let Ok(v) = serde_json::from_str::<Value>(&l) else { continue };
        if codex {
            // the thread is ready: it is the session, and the turn can start
            if v["id"] == 2 {
                if let Some(e) = v["error"]["message"].as_str() {
                    emit(json!({ "type": "error", "text": e }));
                    break;
                }
                let tid = v["result"]["thread"]["id"].as_str().unwrap_or_default();
                emit(json!({ "type": "session", "id": tid }));
                let turn = json!({ "jsonrpc": "2.0", "id": 3, "method": "turn/start", "params": { "threadId": tid, "input": [{ "type": "text", "text": prompt }] } });
                if let Some(w) = stdin.as_mut() {
                    let _ = w.write_all((turn.to_string() + "\n").as_bytes()).await;
                }
                continue;
            }
            // a request we can't answer (approval, user input…): decline instead of hanging the turn
            if v["id"].is_number() && v["method"].is_string() {
                let no = json!({ "jsonrpc": "2.0", "id": v["id"], "error": { "code": -32601, "message": "not supported by traym" } });
                if let Some(w) = stdin.as_mut() {
                    let _ = w.write_all((no.to_string() + "\n").as_bytes()).await;
                }
                continue;
            }
        }
        read(&v).into_iter().for_each(emit);
        if codex && v["method"] == "turn/completed" {
            break;
        }
    }
    let status = match core.agent.lock().await.take() {
        Some(mut c) if codex => {
            let _ = c.kill().await; // app-server stays up waiting for more requests
            None
        }
        Some(mut c) => c.wait().await.ok(),
        None => None, // stopped by the user
    };
    let stderr = err_task.await.unwrap_or_default();
    if status.is_some_and(|s| !s.success()) && !stderr.trim().is_empty() {
        emit(json!({ "type": "error", "text": stderr.trim() }));
    }
    Ok(())
}

pub async fn stop(core: &Core) {
    if let Some(mut c) = core.agent.lock().await.take() {
        let _ = c.kill().await;
    }
    if let Some(t) = core.task.lock().unwrap().take() {
        t.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_clis() {
        let delta = json!({"type":"stream_event","session_id":"s1","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Sal"}}});
        assert_eq!(read(&delta), vec![json!({"type":"session","id":"s1"}), json!({"type":"delta","text":"Sal"})]);
        let tool = json!({"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","name":"mcp__traym__read_note"}}});
        assert_eq!(read(&tool), vec![json!({"type":"tool","name":"read_note"})]);
        // codex app-server
        let d = json!({"method":"item/agentMessage/delta","params":{"delta":"un"}});
        assert_eq!(read(&d), vec![json!({"type":"delta","text":"un"})]);
        let m = json!({"method":"item/completed","params":{"item":{"type":"agentMessage","text":"ok"}}});
        assert_eq!(read(&m), vec![json!({"type":"message","text":"ok"})]);
        let user = json!({"method":"item/completed","params":{"item":{"type":"userMessage"}}});
        assert!(read(&user).is_empty());
        let t = json!({"method":"item/started","params":{"item":{"type":"mcpToolCall","tool":"read_note"}}});
        assert_eq!(read(&t), vec![json!({"type":"tool","name":"read_note"})]);
        assert!(read(&json!({"method":"error","params":{"willRetry":true,"error":{"message":"x"}}})).is_empty());
        let f = json!({"method":"turn/completed","params":{"turn":{"status":"failed","error":{"message":"quota"}}}});
        assert_eq!(read(&f), vec![json!({"type":"error","text":"quota"})]);
    }

    #[test]
    fn model_flags() {
        assert!(claude_args("u", "k", None, Some("opus")).contains(&"opus".to_string()));
        assert!(claude_args("u", "k", None, None).iter().any(|a| a.contains("Bearer k")));
        assert_eq!(pick(&Some("default".into())), None);
        let t = codex_thread("u", "k", "/v", Some("th"), Some("gpt-x"));
        assert_eq!((t["method"].as_str(), t["params"]["threadId"].as_str(), t["params"]["model"].as_str()), (Some("thread/resume"), Some("th"), Some("gpt-x")));
        assert_eq!(codex_thread("u", "k", "/v", None, None)["method"], "thread/start");
        let c = json!({"value":"default","displayName":"Default"});
        assert_eq!(model(false, &c), json!({"id":"default","name":"Default","default":true,"light":false}));
        assert_eq!(model(false, &json!({"value":"haiku"}))["light"], true);
        let x = json!({"model":"gpt-x","displayName":"GPT X","isDefault":true,"description":"Balanced model."});
        assert_eq!(model(true, &x), json!({"id":"gpt-x","name":"GPT X","default":true,"light":false}));
        assert_eq!(model(true, &json!({"model":"gpt-y","description":"Older fast and efficient model."}))["light"], true);
    }
}
