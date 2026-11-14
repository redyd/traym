import { invoke, type InvokeArgs, type InvokeOptions } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { SvelteSet } from 'svelte/reactivity'
import { toast } from 'svelte-sonner'
import { t, i18n } from './i18n.svelte'

export type Node = { name: string; path: string; dir: boolean; children: Node[] }

const ls = {
  get: (k: string) => { try { return localStorage.getItem(k) } catch { return null } },
  set: (k: string, v: string) => { try { localStorage.setItem(k, v) } catch {} },
}

// what closing the window does: ask, or always do one of the popup's choices
export type CloseAction = 'ask' | 'quit' | 'commit' | 'sync'

// accent presets per theme (the light one is darker, for contrast on a light background)
export const ACCENTS: Record<string, { dark: string; light: string }> = {
  orange: { dark: '#ff7a3d', light: '#e8591a' },
  blue: { dark: '#5b9dff', light: '#2563d6' },
  green: { dark: '#4cc38a', light: '#1d8f58' },
  violet: { dark: '#a78bfa', light: '#7346d6' },
  pink: { dark: '#f472b6', light: '#d1367f' },
  red: { dark: '#f87171', light: '#d33a3a' },
}

export const app = $state({
  vault: ls.get('vault') as string | null,
  tree: [] as Node[],
  current: null as string | null,
  open: new SvelteSet<string>(JSON.parse(ls.get('open') ?? '[]')),
  pulse: new SvelteSet<string>(),
  renaming: null as string | null,
  fresh: null as string | null, // just-created note: editor focuses its title
  theme: (ls.get('theme') ?? 'system') as 'system' | 'light' | 'dark',
  density: (ls.get('density') ?? 'normal') as 'compact' | 'normal' | 'large',
  accent: ls.get('accent') ?? 'orange', // a preset name (see ACCENTS) or a #rrggbb colour
  toolbarAlign: (ls.get('toolbarAlign') ?? 'center') as 'center' | 'left',
  pageAlign: (ls.get('pageAlign') ?? 'center') as 'center' | 'left',
  onClose: (['quit', 'commit', 'sync'].includes(ls.get('onClose')!) ? ls.get('onClose') : 'ask') as CloseAction,
  settings: false, // the Settings page replaces the main view while open
  sidebar: ls.get('sidebar') !== '0',
  sidebarW: Number(ls.get('sidebarW') ?? 248),
  agent: ls.get('agentOpen') === '1',
  agentW: Number(ls.get('agentW') ?? 368),
  palette: null as null | 'files' | 'commands',
  git: null as null | { changes: number; remote: string | null; conflicts: string[] },
  syncing: false,
  closing: null as null | 'ask' | 'busy' | { error: string }, // the popup shown while quitting
  pushing: false, // that popup's work includes a push (else only a commit)
})

// set by the editor so file moves never race a pending autosave
export const hooks = { flush: async () => {} }

export const call = <T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions) =>
  invoke<T>(cmd, args, options).catch((e) => {
    flash(String(e))
    throw e
  })

export const flash = (msg: string) => toast.error(msg)

export function persist() {
  if (app.vault) ls.set('vault', app.vault)
  ls.set('theme', app.theme)
  ls.set('density', app.density)
  ls.set('accent', app.accent)
  ls.set('toolbarAlign', app.toolbarAlign)
  ls.set('pageAlign', app.pageAlign)
  ls.set('onClose', app.onClose)
  ls.set('lang', i18n.lang)
  ls.set('sidebar', app.sidebar ? '1' : '0')
  ls.set('sidebarW', String(app.sidebarW))
  ls.set('agentOpen', app.agent ? '1' : '0')
  ls.set('agentW', String(app.agentW))
  ls.set('open', JSON.stringify([...app.open]))
  if (app.current) ls.set('current', app.current)
}

export async function openVault(path: string) {
  await hooks.flush() // pending autosave belongs to the old vault
  app.tree = await call<Node[]>('open_vault', { path })
  app.vault = path
  const last = ls.get('current')
  app.current = last && flat().includes(last) ? last : null
  persist()
  await gitStatus()
  pull()
}

export async function refresh() {
  app.tree = await call<Node[]>('tree')
  await gitStatus()
}

export const gitStatus = async () => (app.git = await invoke<typeof app.git>('git_status').catch(() => null))

export async function gitInit() {
  await call('git_init')
  await gitStatus()
}

export const sync = () => busy(() => call('git_sync'))

async function busy(fn: () => Promise<unknown>) {
  if (app.syncing) return
  app.syncing = true
  try {
    await hooks.flush()
    await fn()
  } finally {
    app.syncing = false
    await refresh().catch(() => {})
  }
}

// opening a vault: bring in what was pushed from elsewhere (a conflict locks the vault, see Conflicts.svelte)
export async function pull() {
  if (app.git?.remote && !app.git.conflicts.length) await busy(() => call('git_pull')).catch(() => {})
}

export const setRemote = (url: string) => busy(() => call('git_set_remote', { url })).then(pull)

export const resolve = (path: string, side: 'ours' | 'theirs' | 'manual') => busy(() => call('git_resolve', { path, side }))

export const abortMerge = () => busy(() => call('git_abort'))

// closing the window: notes are already saved on disk, so the question is commit, or commit and push
// (the latter only offered with a remote). Asked unless a choice was remembered.
// Nothing changed, no git, or mid-conflict: nothing to commit, just close.
// ponytail: commits left unpushed by an earlier "Commit" are not offered here; they go out on the next sync.
export async function quit() {
  if (app.closing === 'busy' || app.closing === 'ask') return
  await hooks.flush()
  await gitStatus()
  if (!app.git?.changes || app.git.conflicts.length) return getCurrentWindow().destroy()
  if (app.onClose === 'ask') app.closing = 'ask'
  else await leave(app.onClose)
}

/** The popup's choice (or the remembered one): commit, push too for sync, then really close. */
export async function leave(action: 'quit' | 'commit' | 'sync', remember = false) {
  if (remember) {
    app.onClose = action
    persist()
  }
  app.pushing = action === 'sync' && !!app.git?.remote
  app.closing = 'busy'
  try {
    await hooks.flush()
    if (action !== 'quit') await invoke('git_sync', { push: action === 'sync' })
    await getCurrentWindow().destroy()
  } catch (e) {
    app.closing = { error: String(e) }
  }
}

export function flat(nodes = app.tree): string[] {
  return nodes.flatMap((n) => (n.dir ? flat(n.children) : [n.path]))
}

export const title = (path: string) => path.split('/').pop()!.replace(/\.md$/, '')
export const parent = (path: string) => path.split('/').slice(0, -1).join('/')
export const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name)

export function exists(path: string, nodes = app.tree): boolean {
  return nodes.some((n) => n.path === path || (n.dir && exists(path, n.children)))
}

function unique(dir: string, base: string, ext: string) {
  let name = `${base}${ext}`
  for (let i = 2; exists(join(dir, name)); i++) name = `${base} ${i}${ext}`
  return join(dir, name)
}

export async function newNote(dir = app.current ? parent(app.current) : '') {
  const path = unique(dir, t('Sans titre'), '.md')
  await call('create_note', { path })
  if (dir) app.open.add(dir)
  await refresh()
  app.fresh = path
  app.current = path
  persist()
}

export async function newFolder(dir = '') {
  const path = unique(dir, t('Nouveau dossier'), '')
  await call('create_folder', { path })
  if (dir) app.open.add(dir)
  await refresh()
  app.renaming = path
}

// one move at a time, each against the tree the previous one left: a drop that fires twice, or a drag that
// started on a row a rename was already moving, finds its source gone and is skipped instead of failing (ENOENT)
let moving = Promise.resolve()
export function move(from: string, to: string) {
  const run = moving.then(() => moveNow(from, to))
  moving = run.catch(() => {})
  return run
}

async function moveNow(from: string, to: string) {
  if (from === to || !exists(from)) return
  await hooks.flush()
  await call('move_path', { from, to })
  // keep the editor and expanded folders pointing at the moved paths
  const remap = (p: string) => (p === from || p.startsWith(from + '/') ? to + p.slice(from.length) : p)
  if (app.current) app.current = remap(app.current)
  app.open = new SvelteSet([...app.open].map(remap))
  await refresh()
  persist()
}

export async function trash(path: string) {
  await hooks.flush()
  await call('trash_path', { path })
  if (app.current && (app.current === path || app.current.startsWith(path + '/'))) app.current = null
  await refresh()
}

export function pulse(path: string) {
  app.pulse.add(path)
  setTimeout(() => app.pulse.delete(path), 1600)
}

// Reveal parents of a path in the tree
export function reveal(path: string) {
  const parts = path.split('/')
  for (let i = 1; i < parts.length; i++) app.open.add(parts.slice(0, i).join('/'))
}

let refreshTimer = 0
export const onDiskChange = (cb: (paths: string[]) => void) =>
  listen<string[]>('vault://changed', (e) => {
    clearTimeout(refreshTimer)
    refreshTimer = window.setTimeout(refresh, 120)
    cb(e.payload)
  })

export const isMac = navigator.platform.toLowerCase().includes('mac')
export const mod = isMac ? '⌘' : 'Ctrl'

// settings the backend keeps (custom AI providers, external MCP access), see src-tauri/src/settings.rs
export type Provider = { id: string; name: string; baseUrl: string; apiKey: string }
export type BackendSettings = { providers: Provider[]; mcpExternal: boolean; mcpToken: string }
export const backend = $state({ settings: { providers: [], mcpExternal: false, mcpToken: '' } as BackendSettings, mcpUrl: '' })

export async function loadSettings() {
  const r = await invoke<{ settings: BackendSettings; mcpUrl: string }>('settings_get')
  backend.settings = r.settings
  backend.mcpUrl = r.mcpUrl
}

export async function saveSettings(s: BackendSettings) {
  backend.settings = await call<BackendSettings>('settings_set', { settings: s })
}
