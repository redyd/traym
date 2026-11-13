<script lang="ts">
  import { FilePlus, FolderPlus, GitBranch, RefreshCw } from '@lucide/svelte'
  import { ContextMenu } from 'bits-ui'
  import Tree from './Tree.svelte'
  import Tip from './Tip.svelte'
  import { app, newNote, newFolder, trash, move, exists, flash, persist, sync, gitInit, type Node } from './store.svelte'

  let node = $state<Node | null>(null) // what the context menu acts on: the item right-clicked, or the vault root
  let over = $state(false)

  const vaultName = $derived(app.vault?.split('/').filter(Boolean).pop() ?? '')

  const onmenu = (_e: MouseEvent, n: Node | null) => (node = n)

  const dirOf = (n: Node | null) => (n ? (n.dir ? n.path : n.path.split('/').slice(0, -1).join('/')) : '')

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

<aside class="sidebar">
  <header>
    <span class="vault" title={app.vault}>{vaultName}</span>
    <Tip class="icon-btn" label="Nouvelle note" onclick={() => newNote()}><FilePlus size={15} strokeWidth={1.5} /></Tip>
    <Tip class="icon-btn" label="Nouveau dossier" onclick={() => newFolder()}><FolderPlus size={15} strokeWidth={1.5} /></Tip>
  </header>

  <ContextMenu.Root>
  <ContextMenu.Trigger>
  {#snippet child({ props })}
  <!-- capture runs before the items' own handler, which then names the item -->
  <div
    {...props}
    class="tree"
    class:over
    role="tree"
    tabindex="-1"
    oncontextmenucapture={() => (node = null)}
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
  {/snippet}
  </ContextMenu.Trigger>
  <ContextMenu.Portal>
    <ContextMenu.Content class="menu" collisionPadding={8} onCloseAutoFocus={(e) => e.preventDefault()}>
      <ContextMenu.Item onSelect={() => newNote(dirOf(node))}>Nouvelle note</ContextMenu.Item>
      <ContextMenu.Item onSelect={() => newFolder(dirOf(node))}>Nouveau dossier</ContextMenu.Item>
      {#if node}
        {@const n = node}
        <ContextMenu.Separator />
        <ContextMenu.Item onSelect={() => (app.renaming = n.path)}>Renommer</ContextMenu.Item>
        <ContextMenu.Item class="danger" onSelect={() => trash(n.path)}>Mettre à la corbeille</ContextMenu.Item>
      {/if}
    </ContextMenu.Content>
  </ContextMenu.Portal>
  </ContextMenu.Root>

  <footer class="git">
    <GitBranch size={14} strokeWidth={1.5} />
    {#if app.git}
      <span class="label">
        {app.git.changes ? `${app.git.changes} modification${app.git.changes > 1 ? 's' : ''}` : 'À jour'}{app.git.remote ? '' : ' · local'}
      </span>
      <Tip class={app.syncing ? 'icon-btn spin' : 'icon-btn'} label="Synchroniser (commit + push)" disabled={app.syncing} onclick={sync}>
        <RefreshCw size={14} strokeWidth={1.5} />
      </Tip>
    {:else}
      <button class="label link" onclick={gitInit}>Activer le suivi git</button>
    {/if}
  </footer>

  <div class="handle" role="separator" aria-orientation="vertical" onpointerdown={resize}></div>
</aside>


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
  :global(.spin svg) { animation: spin 0.9s linear infinite; }
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

</style>
