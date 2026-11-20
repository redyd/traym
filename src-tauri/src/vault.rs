//! The vault is a plain folder of .md files. Every path coming from the UI or
//! from an agent (MCP) goes through `resolve`, which is the trust boundary.

use serde::Serialize;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize, Clone)]
pub struct Node {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub children: Vec<Node>,
}

/// Map a vault-relative path to an absolute one, refusing anything that could
/// land outside the vault (`..`, absolute paths, symlinks pointing out) or in a hidden
/// file or folder: `.git/config` rewritten by an agent would run commands on the next `git status`.
pub fn resolve(root: &Path, rel: &str) -> Res<PathBuf> {
    let p = Path::new(rel);
    if p.components().any(|c| !matches!(c, Component::Normal(n) if !hidden(&n.to_string_lossy()))) {
        return Err(format!("invalid path: {rel}"));
    }
    let full = root.join(p);
    let root_c = root.canonicalize().map_err(err)?;
    let mut probe = full.as_path();
    while fs::symlink_metadata(probe).is_err() {
        probe = probe.parent().unwrap_or(root);
    }
    if !probe.canonicalize().map_err(err)?.starts_with(&root_c) {
        return Err(format!("path outside the vault: {rel}"));
    }
    Ok(full)
}

/// Note paths always end in .md, whatever the caller sends.
pub fn md(rel: &str) -> String {
    if rel.ends_with(".md") { rel.to_string() } else { format!("{rel}.md") }
}

/// Where embedded images are copied: hidden from the tree, committed with the notes.
pub const RESOURCES: &str = ".ressources";

fn hidden(name: &str) -> bool {
    name.starts_with('.')
}

pub fn tree(root: &Path) -> Res<Vec<Node>> {
    walk(root, "")
}

fn walk(dir: &Path, prefix: &str) -> Res<Vec<Node>> {
    let mut out = vec![];
    for e in fs::read_dir(dir).map_err(err)?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if hidden(&name) {
            continue;
        }
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let ft = e.file_type().map_err(err)?;
        if ft.is_dir() {
            let children = walk(&e.path(), &path)?;
            out.push(Node { name, path, dir: true, children });
        } else if ft.is_file() && name.ends_with(".md") {
            out.push(Node { name, path, dir: false, children: vec![] });
        }
    }
    out.sort_by_key(|n| (!n.dir, n.name.to_lowercase()));
    Ok(out)
}

/// Only notes are read and written as text: never another file the vault happens to hold.
fn note(root: &Path, rel: &str) -> Res<PathBuf> {
    if !rel.ends_with(".md") {
        return Err(format!("not a note: {rel}"));
    }
    resolve(root, rel)
}

pub fn read(root: &Path, rel: &str) -> Res<String> {
    fs::read_to_string(note(root, rel)?).map_err(err)
}

pub fn write(root: &Path, rel: &str, content: &str) -> Res<()> {
    let p = note(root, rel)?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    // write-then-rename so a crash mid-save never leaves a truncated note
    let tmp = p.with_extension("md.traym-tmp");
    fs::write(&tmp, content).map_err(err)?;
    fs::rename(&tmp, &p).map_err(err)
}

pub fn create_note(root: &Path, rel: &str, content: &str) -> Res<()> {
    if resolve(root, rel)?.exists() {
        return Err(format!("already exists: {rel}"));
    }
    write(root, rel, content)
}

pub fn create_folder(root: &Path, rel: &str) -> Res<()> {
    fs::create_dir_all(resolve(root, rel)?).map_err(err)
}

pub fn rename(root: &Path, from: &str, to: &str) -> Res<()> {
    let (a, b) = (resolve(root, from)?, resolve(root, to)?);
    if b.exists() {
        return Err(format!("already exists: {to}"));
    }
    // a note stays a note: no turning it into a script or a config file
    if a.is_file() && !(from.ends_with(".md") && to.ends_with(".md")) {
        return Err("only notes (.md) can be moved".into());
    }
    if b.starts_with(&a) {
        return Err("cannot move a folder into itself".into());
    }
    if let Some(parent) = b.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    fs::rename(a, b).map_err(err)
}

/// Deletion goes to the OS trash, so nothing an agent or a misclick does is final.
pub fn trash(root: &Path, rel: &str) -> Res<()> {
    let p = resolve(root, rel)?;
    if p == root {
        return Err("cannot delete the vault".into());
    }
    trash::delete(p).map_err(err)
}

pub fn edit(root: &Path, rel: &str, old: &str, new: &str) -> Res<()> {
    let text = read(root, rel)?;
    match text.matches(old).count() {
        1 => write(root, rel, &text.replacen(old, new, 1)),
        0 => Err("text not found in the note".into()),
        n => Err(format!("text found {n} times, be more specific")),
    }
}

pub fn search(root: &Path, query: &str) -> Res<Vec<String>> {
    let q = query.to_lowercase();
    let mut hits = vec![];
    let mut stack = tree(root)?;
    // ponytail: linear scan of every note, add an index if vaults get huge
    while let Some(n) = stack.pop() {
        if n.dir {
            stack.extend(n.children);
            continue;
        }
        if n.path.to_lowercase().contains(&q) {
            hits.push(n.path.clone());
        }
        for (i, line) in read(root, &n.path).unwrap_or_default().lines().enumerate() {
            if line.to_lowercase().contains(&q) {
                hits.push(format!("{}:{}: {}", n.path, i + 1, line.trim()));
            }
        }
        if hits.len() > 80 {
            break;
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_stays_inside() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir(root.join("a")).unwrap();
        assert!(resolve(root, "a/note.md").is_ok());
        assert!(resolve(root, "new/deep/note.md").is_ok());
        assert!(resolve(root, "../x.md").is_err());
        assert!(resolve(root, "a/../../x.md").is_err());
        assert!(resolve(root, "/etc/passwd").is_err());
        let out = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(out.path(), root.join("link")).unwrap();
        assert!(resolve(root, "link/x.md").is_err());
        assert!(resolve(root, ".git/config").is_err());
        assert!(resolve(root, "a/.hidden/x.md").is_err());
        assert!(read(root, "a/x.txt").is_err());
        assert!(write(root, "x.sh", "").is_err());
        write(root, "evil.md", "").unwrap();
        assert!(rename(root, "evil.md", "evil.sh").is_err());
        assert!(trash(root, ".git").is_err());
    }

    #[test]
    fn ops_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        create_note(root, "Perso/Courses.md", "- [ ] pain").unwrap();
        assert!(create_note(root, "Perso/Courses.md", "").is_err());
        edit(root, "Perso/Courses.md", "pain", "lait").unwrap();
        assert_eq!(read(root, "Perso/Courses.md").unwrap(), "- [ ] lait");
        rename(root, "Perso", "Maison").unwrap();
        assert!(rename(root, "Maison", "Maison/sub").is_err());
        let t = tree(root).unwrap();
        assert_eq!(t[0].children[0].path, "Maison/Courses.md");
        assert_eq!(search(root, "LAIT").unwrap(), vec!["Maison/Courses.md:1: - [ ] lait"]);
    }
}
