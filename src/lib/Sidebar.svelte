<script lang="ts">
  import { FilePlus, FolderPlus, GitBranch, Link, RefreshCw } from '@lucide/svelte'
  import { ContextMenu, Popover } from 'bits-ui'
  import Tree from './Tree.svelte'
  import Tip from './Tip.svelte'
  import { t } from './i18n.svelte'
  import { app, newNote, newFolder, trash, move, exists, flash, persist, sync, gitInit, setRemote, type Node } from './store.svelte'

  let node = $state<Node | null>(null) // what the context menu acts on: the item right-clicked, or the vault root
  let over = $state(false)

  const vaultName = $derived(app.vault?.split('/').filter(Boolean).pop() ?? '')

  let remoteOpen = $state(false)
  let remoteUrl = $state('')
  // `git@github.com:me/notes.git` or `https://github.com/me/notes` → github.com
  const host = (url: string) => url.match(/^(?:[a-z+]+:\/\/)?(?:[^@/]+@)?([^:/]+)/i)?.[1] ?? url

  function saveRemote(url: string) {
    remoteOpen = false
    setRemote(url).catch(() => {})
  }

  const onmenu = (_e: MouseEvent, n: Node | null) => (node = n)

  const dirOf = (n: Node | null) => (n ? (n.dir ? n.path : n.path.split('/').slice(0, -1).join('/')) : '')

  function dropRoot(e: DragEvent) {
    e.preventDefault()
    over = false
    const from = e.dataTransfer?.getData('text/x-traym')
    if (!from || !from.includes('/')) return
    const to = from.split('/').pop()!
    if (exists(to)) return flash(t('Un élément porte déjà ce nom à la racine'))
    move(from, to)
  }

</script>

<aside class="sidebar">
  <header>
    <span class="vault" title={app.vault}>{vaultName}</span>
    <Tip class="icon-btn" label={t('Nouvelle note')} onclick={() => newNote()}><FilePlus size={15} strokeWidth={1.5} /></Tip>
    <Tip class="icon-btn" label={t('Nouveau dossier')} onclick={() => newFolder()}><FolderPlus size={15} strokeWidth={1.5} /></Tip>
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
        <p>{t('Ce dossier ne contient aucune note.')}</p>
        <button onclick={() => newNote('')}>{t('Créer une note')}</button>
      </div>
    {/if}
  </div>
  {/snippet}
  </ContextMenu.Trigger>
  <ContextMenu.Portal>
    <ContextMenu.Content class="menu" collisionPadding={8} onCloseAutoFocus={(e) => e.preventDefault()}>
      <ContextMenu.Item onSelect={() => newNote(dirOf(node))}>{t('Nouvelle note')}</ContextMenu.Item>
      <ContextMenu.Item onSelect={() => newFolder(dirOf(node))}>{t('Nouveau dossier')}</ContextMenu.Item>
      {#if node}
        {@const n = node}
        <ContextMenu.Separator />
        <ContextMenu.Item onSelect={() => (app.renaming = n.path)}>{t('Renommer')}</ContextMenu.Item>
        <ContextMenu.Item class="danger" onSelect={() => trash(n.path)}>{t('Mettre à la corbeille')}</ContextMenu.Item>
      {/if}
    </ContextMenu.Content>
  </ContextMenu.Portal>
  </ContextMenu.Root>

  <footer class="git">
    <GitBranch size={14} strokeWidth={1.5} />
    {#if app.git}
      <span class="label">
        {app.git.changes ? t(app.git.changes > 1 ? '{n} modifications' : '{n} modification', { n: app.git.changes }) : t('À jour')} · {app.git.remote ? host(app.git.remote) : t('local')}
      </span>
      <Popover.Root bind:open={remoteOpen} onOpenChange={(o) => o && (remoteUrl = app.git?.remote ?? '')}>
        <Popover.Trigger>
          {#snippet child({ props })}
            <Tip {...props} class="icon-btn" label={app.git?.remote ?? t('Dépôt distant')}><Link size={14} strokeWidth={1.5} /></Tip>
          {/snippet}
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content class="menu remote" side="top" sideOffset={6} collisionPadding={8}>
            <form onsubmit={(e) => { e.preventDefault(); saveRemote(remoteUrl) }}>
              <label for="remote-url">{t('Dépôt distant')}</label>
              <input id="remote-url" bind:value={remoteUrl} placeholder={t('URL du dépôt')} spellcheck="false" autocomplete="off" />
              <div class="row">
                {#if app.git?.remote}<button type="button" onclick={() => saveRemote('')}>{t('Retirer')}</button>{/if}
                <button type="submit" class="primary" disabled={!remoteUrl.trim()}>{t('Enregistrer')}</button>
              </div>
            </form>
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
      <Tip class={app.syncing ? 'icon-btn spin' : 'icon-btn'} label={t(app.git.remote ? 'Synchroniser (commit + push)' : 'Commit les notes')} disabled={app.syncing} onclick={sync}>
        <RefreshCw size={14} strokeWidth={1.5} />
      </Tip>
    {:else}
      <button class="label link" onclick={gitInit}>{t('Activer le suivi git')}</button>
    {/if}
  </footer>

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
  :global(.remote) { width: 300px; padding: 10px; }
  :global(.remote label) { display: block; margin-bottom: 6px; font: 600 12.5px/1.2 var(--display); color: var(--fg); }
  :global(.remote input) {
    width: 100%;
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--bg);
    color: var(--fg);
    font: 12.5px/1.4 var(--mono);
  }
  :global(.remote input:focus) { outline: none; border-color: var(--orange); }
  :global(.remote .row) { margin-top: 8px; display: flex; justify-content: flex-end; gap: 6px; }
  :global(.remote button) { padding: 6px 10px; border-radius: var(--r); font: 600 12.5px/1 var(--display); color: var(--muted); }
  :global(.remote button:hover) { background: var(--hover); }
  :global(.remote .primary) { background: var(--orange); color: var(--bg); }
  :global(.remote .primary:disabled) { opacity: 0.5; }
  :global(.spin svg) { animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }


</style>
