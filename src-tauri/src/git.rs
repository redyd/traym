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
async fn net(root: &Path, args: &[&str]) -> Res<String> {
    tokio::time::timeout(Duration::from_secs(30), git(root, args))
        .await
        .map_err(|_| "the remote is not responding".to_string())?
}

async fn remote(root: &Path) -> Res<Option<String>> {
    Ok(git(root, &["remote"]).await?.lines().next().map(str::to_string))
}

async fn conflicts(root: &Path) -> Res<Vec<String>> {
    Ok(git(root, &["diff", "--name-only", "--diff-filter=U"]).await?.lines().map(str::to_string).collect())
}

/// None when the vault is not a git repository.
pub async fn status(root: &Path) -> Res<Option<Status>> {
    if git(root, &["rev-parse", "--is-inside-work-tree"]).await.is_err() {
        return Ok(None);
    }
    let changes = git(root, &["status", "--porcelain"]).await?.lines().count();
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

/// Commit everything. Refused mid-conflict: `add -A` would commit the markers.
pub async fn commit(root: &Path) -> Res<()> {
    if !conflicts(root).await?.is_empty() {
        return Err("conflicts need resolving".into());
    }
    git(root, &["add", "-A"]).await?;
    if !git(root, &["status", "--porcelain"]).await?.is_empty() {
        git(root, &["commit", "-m", "traym: sync"]).await?;
    }
    Ok(())
}

pub async fn sync(root: &Path) -> Res<()> {
    commit(root).await?;
    if let Some(r) = remote(root).await? {
        net(root, &["push", "-u", &r, "HEAD"]).await?;
    }
    Ok(())
}

/// Merge the remote's copy of the current branch. A conflict is not an error:
/// it shows up in `status` and locks the vault until resolved.
pub async fn pull(root: &Path) -> Res<()> {
    let Some(r) = remote(root).await? else { return Ok(()) };
    commit(root).await?;
    net(root, &["fetch", &r]).await?;
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
        std::fs::write(root.join("a.md"), "x").unwrap();
        assert_eq!(status(root).await.unwrap().unwrap().changes, 1);
        sync(root).await.unwrap();
        pull(root).await.unwrap(); // no remote: nothing to do
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

        pull(&a).await.unwrap(); // empty remote
        std::fs::write(a.join("a.md"), "base\n").unwrap();
        sync(&a).await.unwrap();

        pull(&c).await.unwrap(); // unborn branch picks up the remote
        assert_eq!(std::fs::read_to_string(c.join("a.md")).unwrap(), "base\n");

        git(d.path(), &["clone", "-q", url, b.to_str().unwrap()]).await.unwrap();
        ident(&b).await;
        std::fs::write(b.join("a.md"), "theirs\n").unwrap();
        sync(&b).await.unwrap();

        std::fs::write(a.join("a.md"), "ours\n").unwrap();
        pull(&a).await.unwrap();
        assert_eq!(status(&a).await.unwrap().unwrap().conflicts, ["a.md"]);
        assert!(commit(&a).await.is_err());
        assert!(resolve(&a, "a.md", "manual").await.is_err());

        abort(&a).await.unwrap();
        assert_eq!(std::fs::read_to_string(a.join("a.md")).unwrap(), "ours\n");
        pull(&a).await.unwrap();
        resolve(&a, "a.md", "theirs").await.unwrap();
        let s = status(&a).await.unwrap().unwrap();
        assert_eq!((s.changes, s.conflicts.len()), (0, 0));
        assert_eq!(std::fs::read_to_string(a.join("a.md")).unwrap(), "theirs\n");
        sync(&a).await.unwrap(); // merge commit pushes cleanly

        set_remote(&a, "").await.unwrap();
        assert_eq!(status(&a).await.unwrap().unwrap().remote, None);
    }
}
