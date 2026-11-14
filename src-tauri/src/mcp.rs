//! Minimal MCP server (JSON-RPC over HTTP POST, no SSE) — same shape as macgram's.
//! claude / codex connect to it to read and write notes in the open vault; with external access
//! on (Settings), other agents can too. Every request needs `Authorization: Bearer <token>`.

use crate::{vault, Core};
use axum::{extract::State, http::HeaderMap, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::Emitter;

pub const INSTRUCTIONS: &str = "traym is a markdown note editor. Notes are .md files in a vault; \
paths are relative to the vault, separated by '/'. Read a note before changing it. \
Prefer edit_note for a targeted change and write_note to rewrite a whole note.";

/// Binds the first free port from 7340.
pub async fn bind() -> (tokio::net::TcpListener, String) {
    for port in 7340..7360 {
        if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            return (l, format!("http://127.0.0.1:{port}/mcp"));
        }
    }
    panic!("no free port for the MCP server (7340-7359)");
}

pub async fn run(l: tokio::net::TcpListener, core: Arc<Core>) {
    let app = Router::new().route("/mcp", post(handle)).with_state(core);
    let _ = axum::serve(l, app).await;
}

async fn handle(State(core): State<Arc<Core>>, headers: HeaderMap, Json(req): Json<Value>) -> axum::response::Response {
    // a browser page always sends Origin; CLIs never do
    if headers.contains_key("origin") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let bearer = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
    if !bearer.is_some_and(|t| core.token_ok(t)) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(id) = req.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    let p = &req["params"];
    let result = match req["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": p["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "traym", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            match run_tool(&core, p["name"].as_str().unwrap_or(""), &p["arguments"]) {
                Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            }
        }
        m => return Json(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("unknown method: {m}") } })).into_response(),
    };
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

/// Run one tool and tell the UI (the chat lists it, the tree pulses the note). Shared with the in-app agent.
pub fn run_tool(core: &Core, name: &str, args: &Value) -> vault::Res<String> {
    let text = call(core, name, args)?;
    let path = args["path"].as_str().or(args["to"].as_str()).map(|s| {
        if name.ends_with("folder") || name == "move_note" { s.to_string() } else { vault::md(s) }
    });
    let _ = core.app.emit("mcp://call", json!({ "tool": name, "path": path }));
    Ok(text)
}

fn s<'a>(args: &'a Value, k: &str) -> vault::Res<&'a str> {
    args[k].as_str().ok_or_else(|| format!("missing argument: {k}"))
}

fn call(core: &Core, name: &str, a: &Value) -> vault::Res<String> {
    let root = core.vault()?;
    let r = root.as_path();
    match name {
        "list_notes" => {
            let mut out = String::new();
            fn dump(nodes: &[vault::Node], depth: usize, out: &mut String) {
                for n in nodes {
                    out.push_str(&format!("{}{}{}\n", "  ".repeat(depth), n.path, if n.dir { "/" } else { "" }));
                    dump(&n.children, depth + 1, out);
                }
            }
            dump(&vault::tree(r)?, 0, &mut out);
            Ok(if out.is_empty() { "(empty vault)".into() } else { out })
        }
        "read_note" => vault::read(r, &vault::md(s(a, "path")?)),
        "write_note" => vault::write(r, &vault::md(s(a, "path")?), s(a, "content")?).map(|_| "ok".into()),
        "create_note" => vault::create_note(r, &vault::md(s(a, "path")?), a["content"].as_str().unwrap_or("")).map(|_| "ok".into()),
        "edit_note" => vault::edit(r, &vault::md(s(a, "path")?), s(a, "old")?, s(a, "new")?).map(|_| "ok".into()),
        "create_folder" => vault::create_folder(r, s(a, "path")?).map(|_| "ok".into()),
        "move_note" => vault::rename(r, s(a, "from")?, s(a, "to")?).map(|_| "ok".into()),
        "delete_note" => vault::trash(r, s(a, "path")?).map(|_| "moved to trash".into()),
        "search_notes" => vault::search(r, s(a, "query")?).map(|h| if h.is_empty() { "no results".into() } else { h.join("\n") }),
        _ => Err(format!("unknown tool: {name}")),
    }
}

pub fn tools() -> Value {
    let path = json!({ "type": "string", "description": "Path relative to the vault, e.g. Projects/ideas.md" });
    let obj = |props: Value, req: &[&str]| json!({ "type": "object", "properties": props, "required": req });
    json!([
        { "name": "list_notes", "description": "Lists the tree of notes and folders in the vault.", "inputSchema": obj(json!({}), &[]) },
        { "name": "read_note", "description": "Reads the markdown content of a note.", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "write_note", "description": "Creates or fully replaces a note.", "inputSchema": obj(json!({ "path": path, "content": { "type": "string" } }), &["path", "content"]) },
        { "name": "create_note", "description": "Creates a new note (fails if it exists).", "inputSchema": obj(json!({ "path": path, "content": { "type": "string" } }), &["path"]) },
        { "name": "edit_note", "description": "Replaces an exact (unique) passage of a note with new text.", "inputSchema": obj(json!({ "path": path, "old": { "type": "string" }, "new": { "type": "string" } }), &["path", "old", "new"]) },
        { "name": "create_folder", "description": "Creates a folder.", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "move_note", "description": "Moves or renames a note or folder.", "inputSchema": obj(json!({ "from": path, "to": path }), &["from", "to"]) },
        { "name": "delete_note", "description": "Moves a note or folder to the trash.", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "search_notes", "description": "Case-insensitive search in note names and contents.", "inputSchema": obj(json!({ "query": { "type": "string" } }), &["query"]) },
    ])
}
