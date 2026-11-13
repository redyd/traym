import { invoke, type InvokeArgs, type InvokeOptions } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { SvelteSet } from 'svelte/reactivity'
import { toast } from 'svelte-sonner'

export type Node = { name: string; path: string; dir: boolean; children: Node[] }

const ls = {
  get: (k: string) => { try { return localStorage.getItem(k) } catch { return null } },
  set: (k: string, v: string) => { try { localStorage.setItem(k, v) } catch {} },
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
  sidebar: ls.get('sidebar') !== '0',
  sidebarW: Number(ls.get('sidebarW') ?? 248),
  agent: ls.get('agentOpen') === '1',
  palette: null as null | 'files' | 'commands',
  git: null as null | { changes: number; remote: boolean },
  syncing: false,
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
  ls.set('sidebar', app.sidebar ? '1' : '0')
  ls.set('sidebarW', String(app.sidebarW))
  ls.set('agentOpen', app.agent ? '1' : '0')
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
  gitStatus()
}

export async function refresh() {
  app.tree = await call<Node[]>('tree')
  gitStatus()
}

export const gitStatus = async () => (app.git = await invoke<typeof app.git>('git_status').catch(() => null))

export async function gitInit() {
  await call('git_init')
  await gitStatus()
}

export async function sync() {
  if (app.syncing) return
  app.syncing = true
  try {
    await hooks.flush()
    await call('git_sync')
  } finally {
    app.syncing = false
    gitStatus()
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
  const path = unique(dir, 'Sans titre', '.md')
  await call('create_note', { path })
  if (dir) app.open.add(dir)
  await refresh()
  app.fresh = path
  app.current = path
  persist()
}

export async function newFolder(dir = '') {
  const path = unique(dir, 'Nouveau dossier', '')
  await call('create_folder', { path })
  if (dir) app.open.add(dir)
  await refresh()
  app.renaming = path
}

export async function move(from: string, to: string) {
  if (from === to) return
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
