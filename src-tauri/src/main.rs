#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod git;
mod mcp;
mod openai;
mod settings;
mod vault;

use notify::{RecursiveMode, Watcher};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Emitter, Manager, State};
use vault::{Node, Res};

pub struct Core {
    pub app: AppHandle,
    pub mcp_url: String,
    vault: RwLock<Option<PathBuf>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    pub agent: tokio::sync::Mutex<Option<tokio::process::Child>>,
    /// the in-app agent's running turn (custom providers), so it can be stopped
    pub task: Mutex<Option<tokio::task::AbortHandle>>,
    /// custom providers' conversations, by session id (the CLIs keep their own)
    pub chats: Mutex<HashMap<String, Vec<Value>>>,
    /// MCP token for the agents traym launches; new on every start
    pub session_token: String,
    settings: RwLock<settings::Settings>,
    settings_path: PathBuf,
}

impl Core {
    pub fn vault(&self) -> Res<PathBuf> {
        self.vault.read().unwrap().clone().ok_or_else(|| "no vault open".into())
    }

    /// MCP access: our own agents always; external ones only while allowed in Settings.
    pub fn token_ok(&self, t: &str) -> bool {
        let s = self.settings.read().unwrap();
        t == self.session_token || (s.mcp_external && !s.mcp_token.is_empty() && t == s.mcp_token)
    }

    pub fn provider(&self, id: &str) -> Res<settings::Provider> {
        let s = self.settings.read().unwrap();
        s.providers.iter().find(|p| p.id == id).cloned().ok_or_else(|| format!("unknown provider: {id}"))
    }
}

type C<'a> = State<'a, Arc<Core>>;

#[tauri::command]
fn open_vault(core: C, path: String) -> Res<Vec<Node>> {
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let root = PathBuf::from(&path).canonicalize().map_err(|e| e.to_string())?;
    let tree = vault::tree(&root)?;
    let app = core.app.clone();
    let base = root.clone();
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(e) = res {
            if e.kind.is_access() {
                return;
            }
            let paths: Vec<String> = e.paths.iter()
                .filter_map(|p| p.strip_prefix(&base).ok())
                .map(|p| p.to_string_lossy().to_string())
                .filter(|p| !p.ends_with(".traym-tmp") && !p.split('/').any(|c| c.starts_with('.')))
                .collect();
            if !paths.is_empty() {
                let _ = app.emit("vault://changed", paths);
            }
        }
    })
    .map_err(|e| e.to_string())?;
    w.watch(&root, RecursiveMode::Recursive).map_err(|e| e.to_string())?;
    *core.watcher.lock().unwrap() = Some(w);
    // the webview may show images from this vault (and only this one) through the asset protocol
    core.app.asset_protocol_scope().allow_directory(&root, true).map_err(|e| e.to_string())?;
    *core.vault.write().unwrap() = Some(root);
    Ok(tree)
}

/// Where a new file named `file` goes in `dir` (vault-relative), next to the note that embeds it:
/// renamed `name-1.ext`… if the name is taken. Returns its vault-relative and absolute paths.
fn free_path(root: &std::path::Path, dir: &str, file: &str) -> Res<(String, PathBuf)> {
    let file = std::path::Path::new(file);
    let stem = file.file_stem().ok_or("invalid file")?.to_string_lossy().to_string();
    let ext = file.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for i in 0.. {
        let name = if i == 0 { format!("{stem}{ext}") } else { format!("{stem}-{i}{ext}") };
        let rel = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        let to = vault::resolve(root, &rel)?;
        if !to.exists() {
            return Ok((rel, to));
        }
    }
    unreachable!()
}

/// Copy a file picked outside the vault into `dir`. Returns its vault-relative path.
#[tauri::command]
fn import_file(core: C, src: String, dir: String) -> Res<String> {
    let from = PathBuf::from(&src);
    let name = from.file_name().ok_or("invalid file")?.to_string_lossy().to_string();
    let (rel, to) = free_path(&core.vault()?, &dir, &name)?;
    std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// Write a pasted or dropped file into `dir`. The bytes come as the raw request body (no JSON),
/// `x-dir` and `x-name` as URI-encoded headers. Returns its vault-relative path.
#[tauri::command]
fn save_file(core: C, request: tauri::ipc::Request) -> Res<String> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else { return Err("invalid content".into()) };
    let header = |k: &str| request.headers().get(k).and_then(|v| v.to_str().ok()).map(uri_decode).ok_or(format!("missing header {k}"));
    let name = header("x-name")?;
    // a bare file name: never a path that could climb out of `dir`
    let name = std::path::Path::new(&name).file_name().ok_or("invalid name")?.to_string_lossy().to_string();
    let (rel, to) = free_path(&core.vault()?, &header("x-dir")?, &name)?;
    std::fs::write(&to, data).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// Undo encodeURIComponent (headers are ASCII only).
fn uri_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b.get(i + 1..i + 3).filter(|_| b[i] == b'%').and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match hex {
            Some(v) => { out.push(v); i += 3 }
            None => { out.push(b[i]); i += 1 }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tauri::command]
fn tree(core: C) -> Res<Vec<Node>> {
    vault::tree(&core.vault()?)
}

#[tauri::command]
fn read_note(core: C, path: String) -> Res<String> {
    vault::read(&core.vault()?, &path)
}

#[tauri::command]
fn write_note(core: C, path: String, content: String) -> Res<()> {
    vault::write(&core.vault()?, &path, &content)
}

#[tauri::command]
fn create_note(core: C, path: String) -> Res<()> {
    vault::create_note(&core.vault()?, &path, "")
}

#[tauri::command]
fn create_folder(core: C, path: String) -> Res<()> {
    vault::create_folder(&core.vault()?, &path)
}

#[tauri::command]
fn move_path(core: C, from: String, to: String) -> Res<()> {
    vault::rename(&core.vault()?, &from, &to)
}

#[tauri::command]
fn trash_path(core: C, path: String) -> Res<()> {
    vault::trash(&core.vault()?, &path)
}

#[tauri::command]
async fn git_status(core: C<'_>) -> Res<Option<git::Status>> {
    git::status(&core.vault()?).await
}

#[tauri::command]
async fn git_init(core: C<'_>) -> Res<()> {
    git::init(&core.vault()?).await
}

#[tauri::command]
async fn git_sync(core: C<'_>, push: Option<bool>) -> Res<()> {
    git::sync(&core.vault()?, push.unwrap_or(true)).await
}

#[tauri::command]
async fn git_set_remote(core: C<'_>, url: String) -> Res<()> {
    git::set_remote(&core.vault()?, &url).await
}

#[tauri::command]
async fn git_pull(core: C<'_>) -> Res<()> {
    git::pull(&core.vault()?).await
}

#[tauri::command]
async fn git_resolve(core: C<'_>, path: String, side: String) -> Res<()> {
    git::resolve(&core.vault()?, &path, &side).await
}

#[tauri::command]
async fn git_abort(core: C<'_>) -> Res<()> {
    git::abort(&core.vault()?).await
}

#[tauri::command]
async fn agent_send(core: C<'_>, agent: String, prompt: String, session: Option<String>, model: Option<String>) -> Res<()> {
    agent::send(core.inner().clone(), agent, prompt, session, model).await
}

#[tauri::command]
fn agent_available() -> serde_json::Value {
    agent::available()
}

#[tauri::command]
async fn agent_models(core: C<'_>, agent: String) -> Res<Vec<Value>> {
    match agent.strip_prefix("custom:") {
        Some(id) => openai::models(core.provider(id)?).await,
        None => agent::models(agent).await,
    }
}

#[tauri::command]
fn settings_get(core: C) -> Value {
    json!({ "settings": *core.settings.read().unwrap(), "mcpUrl": core.mcp_url })
}

/// Saves and returns what was stored (an empty token with external MCP on gets a fresh one).
#[tauri::command]
fn settings_set(core: C, mut settings: settings::Settings) -> Res<settings::Settings> {
    if settings.mcp_external && settings.mcp_token.is_empty() {
        settings.mcp_token = settings::token();
    }
    settings::save(&core.settings_path, &settings)?;
    *core.settings.write().unwrap() = settings.clone();
    Ok(settings)
}

#[tauri::command]
async fn agent_stop(core: C<'_>) -> Res<()> {
    agent::stop(&core).await;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let (listener, url) = tauri::async_runtime::block_on(mcp::bind());
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let core = Arc::new(Core {
                app: app.handle().clone(),
                mcp_url: url,
                vault: RwLock::new(None),
                watcher: Mutex::new(None),
                agent: tokio::sync::Mutex::new(None),
                task: Mutex::new(None),
                chats: Mutex::new(HashMap::new()),
                session_token: settings::token(),
                settings: RwLock::new(settings::load(&settings_path)),
                settings_path,
            });
            tauri::async_runtime::spawn(mcp::run(listener, core.clone()));
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_vault, import_file, save_file, tree, read_note, write_note, create_note, create_folder, move_path, trash_path, git_status, git_init, git_sync, git_set_remote, git_pull, git_resolve, git_abort, agent_send, agent_stop, agent_models, agent_available, settings_get, settings_set
        ])
        .run(tauri::generate_context!())
        .expect("error while starting traym");
}

#[cfg(test)]
mod tests {
    #[test]
    fn uri_decode_undoes_encode_uri_component() {
        assert_eq!(super::uri_decode("capture%20d%E2%80%99%C3%A9cran.png"), "capture d’écran.png");
        assert_eq!(super::uri_decode("plain.png"), "plain.png");
        assert_eq!(super::uri_decode("bad%zz%4"), "bad%zz%4");
    }
}
