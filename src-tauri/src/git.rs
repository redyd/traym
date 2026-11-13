//! Git sync via the user's own `git` binary, so their config, keys and
//! credential helpers apply as-is.

use crate::vault::Res;
use serde::Serialize;
use std::path::Path;
use tokio::process::Command;

#[derive(Serialize)]
pub struct Status {
    pub changes: usize,
    pub remote: bool,
}

async fn git(root: &Path, args: &[&str]) -> Res<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0") // never hang on a password prompt
        .output()
        .await
        .map_err(|e| format!("git introuvable : {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

async fn remote(root: &Path) -> Res<Option<String>> {
    Ok(git(root, &["remote"]).await?.lines().next().map(str::to_string))
}

/// None when the vault is not a git repository.
pub async fn status(root: &Path) -> Res<Option<Status>> {
    if git(root, &["rev-parse", "--is-inside-work-tree"]).await.is_err() {
        return Ok(None);
    }
    let changes = git(root, &["status", "--porcelain"]).await?.lines().count();
    Ok(Some(Status { changes, remote: remote(root).await?.is_some() }))
}

pub async fn init(root: &Path) -> Res<()> {
    git(root, &["init"]).await.map(drop)
}

pub async fn sync(root: &Path) -> Res<()> {
    git(root, &["add", "-A"]).await?;
    if !git(root, &["status", "--porcelain"]).await?.is_empty() {
        git(root, &["commit", "-m", "traym: sync"]).await?;
    }
    if let Some(r) = remote(root).await? {
        git(root, &["push", "-u", &r, "HEAD"]).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_status_sync() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        assert!(status(root).await.unwrap().is_none());
        init(root).await.unwrap();
        git(root, &["config", "user.email", "t@t"]).await.unwrap();
        git(root, &["config", "user.name", "t"]).await.unwrap();
        std::fs::write(root.join("a.md"), "x").unwrap();
        assert_eq!(status(root).await.unwrap().unwrap().changes, 1);
        sync(root).await.unwrap();
        let s = status(root).await.unwrap().unwrap();
        assert_eq!((s.changes, s.remote), (0, false));
    }
}
