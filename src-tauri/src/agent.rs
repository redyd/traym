//! Runs `claude` or `codex` as a one-shot subprocess per turn (ported from macgram).
//! The prompt goes on stdin, stdout is JSONL, normalized into `agent://event`.

use crate::Core;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const BRIEF: &str = "Tu es l'assistant intégré à traym, un éditeur de notes markdown. \
Les notes de l'utilisateur sont des fichiers .md dans un vault : lis-les et modifie-les uniquement \
via les outils MCP traym. Écris un markdown propre et sobre. Réponds brièvement, dans la langue de l'utilisateur.";

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

fn args(agent: &str, mcp: &str, session: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec![];
    if agent == "codex" {
        a.push("exec".into());
        if let Some(s) = session {
            a.extend(["resume".into(), s.into()]);
        }
        for c in [
            "sandbox_mode=\"read-only\"".to_string(),
            format!("mcp_servers.traym.url=\"{mcp}\""),
            "mcp_servers.traym.default_tools_approval_mode=\"approve\"".into(),
            format!("developer_instructions={}", json!(BRIEF)),
        ] {
            a.extend(["-c".into(), c]);
        }
        a.extend(["--json".into(), "--skip-git-repo-check".into(), "-".into()]);
    } else {
        let cfg = json!({ "mcpServers": { "traym": { "type": "http", "url": mcp } } }).to_string();
        for s in ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--strict-mcp-config"] {
            a.push(s.into());
        }
        a.extend(["--mcp-config".into(), cfg, "--allowedTools".into(), "mcp__traym".into()]);
        a.extend(["--append-system-prompt".into(), BRIEF.into()]);
        if let Some(s) = session {
            a.extend(["--resume".into(), s.into()]);
        }
    }
    a
}

/// One CLI JSON line -> zero or more UI events.
fn read(line: &Value) -> Vec<Value> {
    let mut out = vec![];
    let t = line["type"].as_str().unwrap_or("");
    if let Some(s) = line["session_id"].as_str().or(line["thread_id"].as_str()) {
        out.push(json!({ "type": "session", "id": s }));
    }
    match t {
        // claude
        "stream_event" => {
            let e = &line["event"];
            if e["type"] == "content_block_start" && e["content_block"]["type"] == "tool_use" {
                let name = e["content_block"]["name"].as_str().unwrap_or("").trim_start_matches("mcp__traym__");
                out.push(json!({ "type": "tool", "name": name }));
            }
            if e["delta"]["type"] == "text_delta" {
                out.push(json!({ "type": "delta", "text": e["delta"]["text"] }));
            }
        }
        "assistant" => {
            for b in line["message"]["content"].as_array().into_iter().flatten() {
                if b["type"] == "text" {
                    out.push(json!({ "type": "message", "text": b["text"] }));
                }
            }
        }
        "result" if line["is_error"] == true => {
            out.push(json!({ "type": "error", "text": line["result"].as_str().unwrap_or("erreur") }));
        }
        // codex
        "item.completed" if line["item"]["type"] == "agent_message" => {
            out.push(json!({ "type": "message", "text": line["item"]["text"] }));
        }
        "error" => out.push(json!({ "type": "error", "text": line["message"] })),
        "turn.failed" => out.push(json!({ "type": "error", "text": line["error"]["message"] })),
        _ => {}
    }
    out
}

pub async fn send(core: Arc<Core>, agent: String, prompt: String, session: Option<String>) -> Result<(), String> {
    let name = if agent == "codex" { "codex" } else { "claude" };
    let exe = find_cli(name).ok_or_else(|| format!("`{name}` introuvable — installe-le ou ajoute-le au PATH"))?;
    let path = std::env::join_paths(exe.parent().into_iter().map(PathBuf::from).chain(
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default(),
    ))
    .map_err(|e| e.to_string())?;
    let mut child = Command::new(&exe)
        .args(args(name, &core.mcp_url, session.as_deref()))
        .current_dir(core.vault()?)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(prompt.as_bytes()).await.map_err(|e| e.to_string())?;
    drop(stdin);
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
        if let Ok(v) = serde_json::from_str::<Value>(&l) {
            read(&v).into_iter().for_each(emit);
        }
    }
    let status = match core.agent.lock().await.take() {
        Some(mut c) => c.wait().await.ok(),
        None => None, // stopped by the user
    };
    let stderr = err_task.await.unwrap_or_default();
    if status.is_some_and(|s| !s.success()) && !stderr.trim().is_empty() {
        emit(json!({ "type": "error", "text": stderr.trim() }));
    }
    emit(json!({ "type": "done" }));
    Ok(())
}

pub async fn stop(core: &Core) {
    if let Some(mut c) = core.agent.lock().await.take() {
        let _ = c.kill().await;
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
        let codex = json!({"type":"item.completed","item":{"type":"agent_message","text":"ok"}});
        assert_eq!(read(&codex), vec![json!({"type":"message","text":"ok"})]);
        assert_eq!(read(&json!({"type":"thread.started","thread_id":"t"})), vec![json!({"type":"session","id":"t"})]);
    }
}
