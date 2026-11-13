#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod git;
mod mcp;
mod vault;

use notify::{RecursiveMode, Watcher};
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
}

impl Core {
    pub fn vault(&self) -> Res<PathBuf> {
        self.vault.read().unwrap().clone().ok_or_else(|| "aucun vault ouvert".into())
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
    *core.vault.write().unwrap() = Some(root);
    Ok(tree)
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
async fn git_sync(core: C<'_>) -> Res<()> {
    git::sync(&core.vault()?).await
}

#[tauri::command]
async fn agent_send(core: C<'_>, agent: String, prompt: String, session: Option<String>) -> Res<()> {
    agent::send(core.inner().clone(), agent, prompt, session).await
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
            let core = Arc::new(Core {
                app: app.handle().clone(),
                mcp_url: url,
                vault: RwLock::new(None),
                watcher: Mutex::new(None),
                agent: tokio::sync::Mutex::new(None),
            });
            tauri::async_runtime::spawn(mcp::run(listener, core.clone()));
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_vault, tree, read_note, write_note, create_note, create_folder, move_path, trash_path, git_status, git_init, git_sync, agent_send, agent_stop
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de traym");
}
