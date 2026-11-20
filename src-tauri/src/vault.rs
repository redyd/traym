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
/// land outside the vault (`..`, absolute paths, symlinks pointing out) or in `.git`:
/// `.git/config` rewritten by an agent would run commands on the next `git status`.
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

/// Where embedded images are copied, committed with the notes.
pub const RESOURCES: &str = ".ressources";

/// The only things of the vault folder the app never shows nor touches: git's own folder (any case:
/// macOS and Windows disks ignore it) and the temp files of a save in progress.
pub fn hidden(name: &str) -> bool {
    name.eq_ignore_ascii_case(".git") || name.ends_with(".traym-tmp")
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
        } else if ft.is_file() {
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
    save(&p, content)
}

/// write-then-rename so a crash mid-save never leaves a truncated file
fn save(p: &Path, content: &str) -> Res<()> {
    // `hidden`, so the watcher and the tree never see it
    let tmp = p.with_file_name(format!(".{}.traym-tmp", p.file_name().ok_or("invalid path")?.to_string_lossy()));
    fs::write(&tmp, content).map_err(err)?;
    fs::rename(&tmp, p).map_err(err)
}

/// Any other file of the vault, opened from the tree: read as text when it is text.
pub fn read_text(root: &Path, rel: &str) -> Res<String> {
    let p = resolve(root, rel)?;
    // ponytail: 5 MB cap for the plain textarea, a virtualised view if big logs matter
    if fs::metadata(&p).map_err(err)?.len() > 5 << 20 {
        return Err(format!("too large to open: {rel}"));
    }
    String::from_utf8(fs::read(&p).map_err(err)?).map_err(|_| format!("not a text file: {rel}"))
}

/// Saves a file opened from the tree. Only one that already exists: the UI never creates scripts.
pub fn write_text(root: &Path, rel: &str, content: &str) -> Res<()> {
    let p = resolve(root, rel)?;
    if !p.is_file() {
        return Err(format!("no such file: {rel}"));
    }
    save(&p, content)
}

pub fn create_note(root: &Path, rel: &str, content: &str) -> Res<()> {
    if resolve(root, rel)?.exists() {
        return Err(format!("already exists: {rel}"));
    }
    write(root, rel, content)
}

/// Copy a file pasted from the system's file manager into the vault folder `dir` (renamed `name 2.ext`…
/// if taken). Returns its vault-relative path. Nothing from a dot folder or file (`~/.ssh/…`):
/// the webview names the source, and anything copied in can be read back through `read_text`.
pub fn import(root: &Path, src: &Path, dir: &str) -> Res<String> {
    if !src.is_absolute() || src.components().any(|c| matches!(c, Component::Normal(n) if n.to_string_lossy().starts_with('.'))) {
        return Err(format!("cannot import {}", src.display()));
    }
    if !src.is_file() {
        return Err(format!("only files can be imported: {}", src.display()));
    }
    resolve(root, dir)?;
    let stem = src.file_stem().ok_or("invalid file")?.to_string_lossy().to_string();
    let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for i in 1.. {
        let name = if i == 1 { format!("{stem}{ext}") } else { format!("{stem} {i}{ext}") };
        let rel = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        let to = resolve(root, &rel)?;
        if !to.exists() {
            fs::copy(src, &to).map_err(err)?;
            return Ok(rel);
        }
    }
    unreachable!()
}

pub fn create_folder(root: &Path, rel: &str) -> Res<()> {
    fs::create_dir_all(resolve(root, rel)?).map_err(err)
}

pub fn rename(root: &Path, from: &str, to: &str) -> Res<()> {
    let (a, b) = (resolve(root, from)?, resolve(root, to)?);
    if b.exists() {
        return Err(format!("already exists: {to}"));
    }
    // a note stays a note (agents may move notes): no turning one into a script or a config file
    if a.is_file() && from.ends_with(".md") && !to.ends_with(".md") {
        return Err("a note keeps its .md extension".into());
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
        assert!(resolve(root, ".GIT/hooks/x").is_err());
        assert!(resolve(root, "a/.hidden/x.md").is_ok());
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

    #[test]
    fn other_files() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::write(root.join("a.txt"), "hello").unwrap();
        fs::write(root.join("b.bin"), [0xff, 0xfe, 0x00]).unwrap();
        let names: Vec<_> = tree(root).unwrap().into_iter().map(|n| n.name).collect();
        assert_eq!(names, ["a.txt", "b.bin"]);
        assert_eq!(read_text(root, "a.txt").unwrap(), "hello");
        assert!(read_text(root, "b.bin").is_err());
        write_text(root, "a.txt", "bye").unwrap();
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "bye");
        assert!(write_text(root, "new.sh", "").is_err());
        rename(root, "a.txt", "LICENSE").unwrap(); // other files: renamed freely
        rename(root, "LICENSE", "c.txt").unwrap();
        assert_eq!(tree(root).unwrap().len(), 2); // the save's temp file is gone, and was hidden anyway

        let out = tempfile::Builder::new().prefix("out").tempdir().unwrap(); // the default name is hidden
        fs::write(out.path().join("doc.pdf"), "x").unwrap();
        fs::create_dir(out.path().join(".ssh")).unwrap();
        fs::write(out.path().join(".ssh/id"), "secret").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        assert_eq!(import(root, &out.path().join("doc.pdf"), "sub").unwrap(), "sub/doc.pdf");
        assert_eq!(import(root, &out.path().join("doc.pdf"), "sub").unwrap(), "sub/doc 2.pdf");
        assert_eq!(import(root, &out.path().join("doc.pdf"), "").unwrap(), "doc.pdf");
        assert!(import(root, &out.path().join(".ssh/id"), "").is_err());
        assert!(import(root, out.path(), "").is_err()); // a folder
        assert!(import(root, &out.path().join("doc.pdf"), "../x").is_err());
    }
}
