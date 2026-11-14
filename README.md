# traym

A desktop markdown note editor with git sync and a built-in AI assistant. Built with Tauri 2, Svelte 5 and Tiptap.

## Features

- **Notes are plain `.md` files** in a folder (the vault, `~/Documents/traym` by default).
- **Rich editor**: tables, images (paste or drop), custom context menu, smooth caret.
- **Git sync**: optional remote, pull on start, commit / push on close, conflict resolution in the app. Only notes (`*.md`) are committed.
- **AI panel**: Claude Code, Codex or any OpenAI-compatible API, working on your notes through a local MCP server (token-protected, external access opt-in).
- **Customizable**: French / English, light / dark, accent colour that themes the whole UI, density, resizable panels.

## Install

Download the build for your system from the [releases](../../releases): AppImage or RPM (Linux), installer (Windows), DMG (macOS).

`git` must be installed for sync. The AI panel uses the `claude` / `codex` CLIs if they are on your machine.

## Development

Requirements: Node 22+, Rust stable, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
npm install
npm run tauri dev     # run the app
npm run check         # type-check the frontend
cd src-tauri && cargo test
```

## Release

Push a tag starting with `v`: GitHub Actions builds every platform and publishes a release.

```sh
git tag v0.2.0 && git push origin v0.2.0
```

Keep `version` in `src-tauri/tauri.conf.json` in step with the tag.
