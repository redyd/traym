<script lang="ts">
  import { FilePlus, FolderPlus, GitBranch, RefreshCw } from '@lucide/svelte'
  import Tree from './Tree.svelte'
  import { app, newNote, newFolder, trash, move, exists, flash, persist, sync, gitInit, type Node } from './store.svelte'

  let menu = $state<{ x: number; y: number; node: Node | null } | null>(null)
  let over = $state(false)

  const vaultName = $derived(app.vault?.split('/').filter(Boolean).pop() ?? '')

  function onmenu(e: MouseEvent, node: Node | null) {
    e.preventDefault()
    e.stopPropagation()
    menu = { x: Math.min(e.clientX, innerWidth - 200), y: Math.min(e.clientY, innerHeight - 180), node }
  }

  const dirOf = (n: Node | null) => (n ? (n.dir ? n.path : n.path.split('/').slice(0, -1).join('/')) : '')

  function act(fn: () => unknown) {
    menu = null
    fn()
  }

  function dropRoot(e: DragEvent) {
    e.preventDefault()
    over = false
    const from = e.dataTransfer?.getData('text/x-traym')
    if (!from || !from.includes('/')) return
    const to = from.split('/').pop()!
    if (exists(to)) return flash('Un élément porte déjà ce nom à la racine')
    move(from, to)
  }

  function resize(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement
    el.setPointerCapture(e.pointerId)
    const move = (ev: PointerEvent) => (app.sidebarW = Math.max(180, Math.min(420, ev.clientX)))
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', () => { el.removeEventListener('pointermove', move); persist() }, { once: true })
  }
</script>

<svelte:window onclick={() => (menu = null)} onkeydown={(e) => e.key === 'Escape' && (menu = null)} onblur={() => (menu = null)} />

<aside class="sidebar">
  <header>
    <span class="vault" title={app.vault}>{vaultName}</span>
    <button class="icon-btn" title="Nouvelle note" onclick={() => newNote()}><FilePlus size={15} strokeWidth={1.5} /></button>
    <button class="icon-btn" title="Nouveau dossier" onclick={() => newFolder()}><FolderPlus size={15} strokeWidth={1.5} /></button>
  </header>

  <div
    class="tree"
    class:over
    role="tree"
    tabindex="-1"
    oncontextmenu={(e) => onmenu(e, null)}
    ondragover={(e) => { e.preventDefault(); over = true }}
    ondragleave={() => (over = false)}
    ondrop={dropRoot}
  >
    {#if app.tree.length}
      <Tree nodes={app.tree} {onmenu} />
    {:else}
      <div class="empty">
        <p>Ce dossier ne contient aucune note.</p>
        <button onclick={() => newNote('')}>Créer une note</button>
      </div>
    {/if}
  </div>

  <footer class="git">
    <GitBranch size={14} strokeWidth={1.5} />
    {#if app.git}
      <span class="label">
        {app.git.changes ? `${app.git.changes} modification${app.git.changes > 1 ? 's' : ''}` : 'À jour'}{app.git.remote ? '' : ' · local'}
      </span>
      <button class="icon-btn" class:spin={app.syncing} title="Synchroniser (commit + push)" disabled={app.syncing} onclick={sync}>
        <RefreshCw size={14} strokeWidth={1.5} />
      </button>
    {:else}
      <button class="label link" onclick={gitInit}>Activer le suivi git</button>
    {/if}
  </footer>

  <div class="handle" role="separator" aria-orientation="vertical" onpointerdown={resize}></div>
</aside>

{#if menu}
  {@const n = menu.node}
  <div class="menu" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
    <button role="menuitem" onclick={() => act(() => newNote(dirOf(n)))}>Nouvelle note</button>
    <button role="menuitem" onclick={() => act(() => newFolder(dirOf(n)))}>Nouveau dossier</button>
    {#if n}
      <hr />
      <button role="menuitem" onclick={() => act(() => (app.renaming = n.path))}>Renommer</button>
      <button role="menuitem" class="danger" onclick={() => act(() => trash(n.path))}>Mettre à la corbeille</button>
    {/if}
  </div>
{/if}

<style>
  .sidebar {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-deep);
    border-right: 1px solid var(--line-soft);
    min-width: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 10px 8px 8px 16px;
  }
  .vault {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: 650 13px/1 var(--display);
    color: var(--fg);
    letter-spacing: -0.005em;
  }
  .tree {
    flex: 1;
    overflow: auto;
    padding: 2px 0 24px;
    border-radius: var(--r);
    transition: box-shadow var(--t-fast) var(--ease);
  }
  .tree.over { box-shadow: inset 0 0 0 1px var(--line); }
  .empty { padding: 12px 16px; color: var(--faint); }
  .empty p { margin: 0 0 10px; }
  .empty button { color: var(--orange); font-weight: 600; }
  .empty button:hover { text-decoration: underline; text-underline-offset: 3px; }

  .git {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 8px 16px;
    border-top: 1px solid var(--line-soft);
    color: var(--faint);
    font: 500 12px/1 var(--display);
  }
  .git .label { flex: 1; min-width: 0; text-align: left; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .git .link:hover { color: var(--orange); }
  .spin :global(svg) { animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .handle {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: col-resize;
    z-index: 3;
  }
  .handle:hover, .handle:active { background: linear-gradient(var(--line), var(--line)) center / 1px 100% no-repeat; }

  .menu {
    position: fixed;
    z-index: 50;
    min-width: 188px;
    padding: 4px;
    background: var(--surface-solid);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 12px 32px -12px #0000008c;
    animation: menu 140ms var(--ease);
    transform-origin: top left;
  }
  @keyframes menu { from { opacity: 0; transform: scale(.97); } }
  .menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 6px 10px;
    border-radius: 4px;
    color: var(--fg);
  }
  .menu button:hover { background: var(--orange-wash); }
  .menu .danger:hover { color: var(--orange); }
  .menu hr { border: 0; border-top: 1px solid var(--line-soft); margin: 4px 2px; }
</style>
