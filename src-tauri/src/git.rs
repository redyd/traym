//! Git sync via the user's own `git` binary, so their config, keys and
//! credential helpers apply as-is. A remote is optional: every network step
//! is skipped when the vault has none.

use crate::vault::{self, Res};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

#[derive(Serialize)]
pub struct Status {
    pub changes: usize,
    /// url of the remote, if any
    pub remote: Option<String>,
    /// unmerged paths left by a pull: the vault stays locked until they are resolved
    pub conflicts: Vec<String>,
}

async fn git(root: &Path, args: &[&str]) -> Res<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0") // never hang on a password prompt
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("git not found: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Network commands: bounded, so closing the app never waits forever on a dead remote.
async fn net(root: &Path, args: &[&str], secs: u64) -> Res<String> {
    tokio::time::timeout(Duration::from_secs(secs), git(root, args))
        .await
        .map_err(|_| "the remote is not responding".to_string())?
}

async fn remote(root: &Path) -> Res<Option<String>> {
    Ok(git(root, &["remote"]).await?.lines().next().map(str::to_string))
}

async fn conflicts(root: &Path) -> Res<Vec<String>> {
    Ok(git(root, &["diff", "--name-only", "--diff-filter=U"]).await?.lines().map(str::to_string).collect())
}

/// Everything in the vault folder (minus what a .gitignore excludes) is synced: `.` is the vault,
/// even when it sits inside a bigger repository.
const SYNCED: &[&str] = &["."];

/// Files under `specs` added, edited, moved or deleted since the last commit, one per line.
async fn changes(root: &Path, specs: &[&str]) -> Res<String> {
    git(root, &[&["status", "--porcelain", "-uall", "--"], specs].concat()).await
}

/// None when the vault is not a git repository.
pub async fn status(root: &Path) -> Res<Option<Status>> {
    if git(root, &["rev-parse", "--is-inside-work-tree"]).await.is_err() {
        return Ok(None);
    }
    let changes = changes(root, SYNCED).await?.lines().count();
    let remote = match remote(root).await? {
        Some(r) => Some(git(root, &["remote", "get-url", &r]).await?.trim().to_string()),
        None => None,
    };
    Ok(Some(Status { changes, remote, conflicts: conflicts(root).await? }))
}

pub async fn init(root: &Path) -> Res<()> {
    git(root, &["init", "-b", "main"]).await.map(drop)
}

/// Empty url removes the remote.
pub async fn set_remote(root: &Path, url: &str) -> Res<()> {
    let url = url.trim();
    if url.starts_with('-') {
        return Err("invalid url".into());
    }
    match (remote(root).await?, url.is_empty()) {
        (Some(r), true) => git(root, &["remote", "remove", &r]).await,
        (Some(r), false) => git(root, &["remote", "set-url", &r, url]).await,
        (None, false) => git(root, &["remote", "add", "origin", url]).await,
        (None, true) => Ok(String::new()),
    }
    .map(drop)
}

pub const DEFAULT_MESSAGE: &str = "traym: sync";

/// Commit the notes (an empty message means the default one). Refused mid-conflict: `add` would commit the markers.
pub async fn commit(root: &Path, message: &str) -> Res<()> {
    let message = if message.trim().is_empty() { DEFAULT_MESSAGE } else { message.trim() };
    if !conflicts(root).await?.is_empty() {
        return Err("conflicts need resolving".into());
    }
    // no change, no add: `add` fails on a pathspec that matches nothing (an empty vault)
    if !changes(root, SYNCED).await?.is_empty() {
        git(root, &[&["add", "-A", "--"], SYNCED].concat()).await?;
        git(root, &["commit", "-m", message]).await?;
    }
    Ok(())
}

/// Commit the notes; with `push`, also send it to the remote when there is one.
pub async fn sync(root: &Path, push: bool, message: &str) -> Res<()> {
    commit(root, message).await?;
    if let Some(r) = remote(root).await?.filter(|_| push) {
        net(root, &["push", "-u", &r, "HEAD"], 30).await?;
    }
    Ok(())
}

/// Merge the remote's copy of the current branch. A conflict is not an error:
/// it shows up in `status` and locks the vault until resolved.
/// `secs` bounds the fetch: shorter at startup, where the boot screen waits on it.
pub async fn pull(root: &Path, secs: u64) -> Res<()> {
    let Some(r) = remote(root).await? else { return Ok(()) };
    commit(root, "").await?;
    net(root, &["fetch", &r], secs).await?;
    let branch = git(root, &["symbolic-ref", "--short", "HEAD"]).await?;
    let theirs = format!("{r}/{}", branch.trim());
    if git(root, &["rev-parse", "--verify", "--quiet", &theirs]).await.is_err() {
        return Ok(()); // remote is empty or lacks this branch: the next push creates it
    }
    match git(root, &["merge", "--no-edit", "--allow-unrelated-histories", &theirs]).await {
        Err(e) if conflicts(root).await?.is_empty() => Err(e),
        _ => Ok(()),
    }
}

/// `side`: "ours" / "theirs" keeps one whole version, "manual" accepts the file
/// as edited by hand once no conflict marker is left. Concludes the merge after the last one.
pub async fn resolve(root: &Path, path: &str, side: &str) -> Res<()> {
    match side {
        "ours" | "theirs" => {
            // modify/delete: the kept side has no file, so the resolution is the deletion
            if git(root, &["checkout", &format!("--{side}"), "--", path]).await.is_err() {
                git(root, &["rm", "--quiet", "--", path]).await?;
            }
        }
        "manual" => {
            let text = std::fs::read_to_string(vault::resolve(root, path)?).unwrap_or_default();
            if text.lines().any(|l| l.starts_with("<<<<<<<") || l.starts_with(">>>>>>>")) {
                return Err(format!("{path} still contains conflict markers"));
            }
        }
        _ => return Err("unknown resolution".into()),
    }
    git(root, &["add", "-A", "--", path]).await?;
    if conflicts(root).await?.is_empty() {
        git(root, &["commit", "--no-edit"]).await?;
    }
    Ok(())
}

pub async fn abort(root: &Path) -> Res<()> {
    git(root, &["merge", "--abort"]).await.map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ident(root: &Path) {
        git(root, &["config", "user.email", "t@t"]).await.unwrap();
        git(root, &["config", "user.name", "t"]).await.unwrap();
    }

    #[tokio::test]
    async fn init_status_sync() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        assert!(status(root).await.unwrap().is_none());
        init(root).await.unwrap();
        ident(root).await;
        sync(root, true, "").await.unwrap(); // nothing to commit, even with no file at all
        std::fs::create_dir(root.join("New folder")).unwrap();
        std::fs::write(root.join("New folder/a.md"), "x").unwrap();
        std::fs::write(root.join("New folder/b"), "x").unwrap(); // any file of the vault: synced
        std::fs::write(root.join(".env"), "x").unwrap(); // dot files too
        assert_eq!(status(root).await.unwrap().unwrap().changes, 3);
        sync(root, true, "  ma note  ").await.unwrap();
        assert_eq!(git(root, &["ls-files"]).await.unwrap(), ".env\nNew folder/a.md\nNew folder/b\n");
        assert_eq!(git(root, &["log", "-1", "--format=%s"]).await.unwrap(), "ma note\n");
        std::fs::create_dir(root.join(vault::RESOURCES)).unwrap();
        std::fs::write(root.join(".ressources/x.png"), "x").unwrap(); // an embedded image: committed alone
        assert_eq!(status(root).await.unwrap().unwrap().changes, 1);
        sync(root, true, "").await.unwrap();
        assert_eq!(git(root, &["ls-files"]).await.unwrap(), ".env\n.ressources/x.png\nNew folder/a.md\nNew folder/b\n");
        std::fs::remove_dir_all(root.join(vault::RESOURCES)).unwrap();
        std::fs::remove_file(root.join("New folder/a.md")).unwrap();
        std::fs::remove_file(root.join("New folder/b")).unwrap();
        std::fs::remove_file(root.join(".env")).unwrap();
        sync(root, true, "").await.unwrap(); // a deleted note is committed too
        assert_eq!(git(root, &["ls-files"]).await.unwrap(), "");
        assert_eq!(git(root, &["log", "-1", "--format=%s"]).await.unwrap(), "traym: sync\n");
        pull(root, 30).await.unwrap(); // no remote: nothing to do
        let s = status(root).await.unwrap().unwrap();
        assert_eq!((s.changes, s.remote, s.conflicts.len()), (0, None, 0));
    }

    #[tokio::test]
    async fn remote_pull_conflict_resolve() {
        let d = tempfile::tempdir().unwrap();
        let (bare, a, b, c) = (d.path().join("r.git"), d.path().join("a"), d.path().join("b"), d.path().join("c"));
        let url = bare.to_str().unwrap();
        git(d.path(), &["init", "--bare", "-b", "main", url]).await.unwrap();
        for v in [&a, &c] {
            std::fs::create_dir(v).unwrap();
            init(v).await.unwrap();
            ident(v).await;
            set_remote(v, url).await.unwrap();
        }
        assert!(set_remote(&a, "--upload-pack=x").await.is_err());
        assert_eq!(status(&a).await.unwrap().unwrap().remote.as_deref(), Some(url));

        pull(&a, 30).await.unwrap(); // empty remote
        std::fs::write(a.join("a.md"), "base\n").unwrap();
        sync(&a, false, "").await.unwrap(); // save only: committed here, nothing on the remote
        assert!(git(d.path(), &["ls-remote", url]).await.unwrap().trim().is_empty());
        assert!(status(&a).await.unwrap().unwrap().changes == 0);
        sync(&a, true, "").await.unwrap();
        assert!(!git(d.path(), &["ls-remote", url]).await.unwrap().trim().is_empty());

        pull(&c, 30).await.unwrap(); // unborn branch picks up the remote
        assert_eq!(std::fs::read_to_string(c.join("a.md")).unwrap(), "base\n");

        git(d.path(), &["clone", "-q", url, b.to_str().unwrap()]).await.unwrap();
        ident(&b).await;
        std::fs::write(b.join("a.md"), "theirs\n").unwrap();
        sync(&b, true, "").await.unwrap();

        std::fs::write(a.join("a.md"), "ours\n").unwrap();
        pull(&a, 30).await.unwrap();
        assert_eq!(status(&a).await.unwrap().unwrap().conflicts, ["a.md"]);
        assert!(commit(&a, "").await.is_err());
        assert!(resolve(&a, "a.md", "manual").await.is_err());

        abort(&a).await.unwrap();
        assert_eq!(std::fs::read_to_string(a.join("a.md")).unwrap(), "ours\n");
        pull(&a, 30).await.unwrap();
        resolve(&a, "a.md", "theirs").await.unwrap();
        let s = status(&a).await.unwrap().unwrap();
        assert_eq!((s.changes, s.conflicts.len()), (0, 0));
        assert_eq!(std::fs::read_to_string(a.join("a.md")).unwrap(), "theirs\n");
        sync(&a, true, "").await.unwrap(); // merge commit pushes cleanly

        set_remote(&a, "").await.unwrap();
        assert_eq!(status(&a).await.unwrap().unwrap().remote, None);
    }
}
