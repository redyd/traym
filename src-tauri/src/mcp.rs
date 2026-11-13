//! Minimal MCP server (JSON-RPC over HTTP POST, no SSE) — same shape as macgram's.
//! claude / codex connect to it to read and write notes in the open vault.

use crate::{vault, Core};
use axum::{extract::State, http::HeaderMap, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::Emitter;

const INSTRUCTIONS: &str = "traym est un éditeur de notes markdown. Les notes sont des fichiers .md \
dans un vault ; les chemins sont relatifs au vault, séparés par '/'. Lis une note avant de la modifier. \
Préfère edit_note pour une modification ciblée et write_note pour réécrire une note entière.";

/// Binds the first free port from 7340.
pub async fn bind() -> (tokio::net::TcpListener, String) {
    for port in 7340..7360 {
        if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            return (l, format!("http://127.0.0.1:{port}/mcp"));
        }
    }
    panic!("aucun port libre pour le serveur MCP (7340-7359)");
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
            let name = p["name"].as_str().unwrap_or("");
            let args = &p["arguments"];
            match call(&core, name, args) {
                Ok(text) => {
                    let path = args["path"].as_str().or(args["to"].as_str()).map(|s| {
                        if name.ends_with("folder") || name == "move_note" { s.to_string() } else { vault::md(s) }
                    });
                    let _ = core.app.emit("mcp://call", json!({ "tool": name, "path": path }));
                    json!({ "content": [{ "type": "text", "text": text }] })
                }
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            }
        }
        m => return Json(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("méthode inconnue : {m}") } })).into_response(),
    };
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

fn s<'a>(args: &'a Value, k: &str) -> vault::Res<&'a str> {
    args[k].as_str().ok_or_else(|| format!("argument manquant : {k}"))
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
            Ok(if out.is_empty() { "(vault vide)".into() } else { out })
        }
        "read_note" => vault::read(r, &vault::md(s(a, "path")?)),
        "write_note" => vault::write(r, &vault::md(s(a, "path")?), s(a, "content")?).map(|_| "ok".into()),
        "create_note" => vault::create_note(r, &vault::md(s(a, "path")?), a["content"].as_str().unwrap_or("")).map(|_| "ok".into()),
        "edit_note" => vault::edit(r, &vault::md(s(a, "path")?), s(a, "old")?, s(a, "new")?).map(|_| "ok".into()),
        "create_folder" => vault::create_folder(r, s(a, "path")?).map(|_| "ok".into()),
        "move_note" => vault::rename(r, s(a, "from")?, s(a, "to")?).map(|_| "ok".into()),
        "delete_note" => vault::trash(r, s(a, "path")?).map(|_| "déplacé dans la corbeille".into()),
        "search_notes" => vault::search(r, s(a, "query")?).map(|h| if h.is_empty() { "aucun résultat".into() } else { h.join("\n") }),
        _ => Err(format!("outil inconnu : {name}")),
    }
}

fn tools() -> Value {
    let path = json!({ "type": "string", "description": "Chemin relatif au vault, ex. Projets/idées.md" });
    let obj = |props: Value, req: &[&str]| json!({ "type": "object", "properties": props, "required": req });
    json!([
        { "name": "list_notes", "description": "Liste l'arborescence des notes et dossiers du vault.", "inputSchema": obj(json!({}), &[]) },
        { "name": "read_note", "description": "Lit le contenu markdown d'une note.", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "write_note", "description": "Crée ou remplace entièrement une note.", "inputSchema": obj(json!({ "path": path, "content": { "type": "string" } }), &["path", "content"]) },
        { "name": "create_note", "description": "Crée une nouvelle note (échoue si elle existe).", "inputSchema": obj(json!({ "path": path, "content": { "type": "string" } }), &["path"]) },
        { "name": "edit_note", "description": "Remplace un passage exact (unique) d'une note par un nouveau texte.", "inputSchema": obj(json!({ "path": path, "old": { "type": "string" }, "new": { "type": "string" } }), &["path", "old", "new"]) },
        { "name": "create_folder", "description": "Crée un dossier.", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "move_note", "description": "Déplace ou renomme une note ou un dossier.", "inputSchema": obj(json!({ "from": path, "to": path }), &["from", "to"]) },
        { "name": "delete_note", "description": "Met une note ou un dossier à la corbeille.", "inputSchema": obj(json!({ "path": path }), &["path"]) },
        { "name": "search_notes", "description": "Recherche (insensible à la casse) dans les noms et le contenu des notes.", "inputSchema": obj(json!({ "query": { "type": "string" } }), &["query"]) },
    ])
}
