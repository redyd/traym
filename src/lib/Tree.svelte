<script lang="ts">
  import { t } from './i18n.svelte'
  import { ChevronRight, Folder, FolderOpen } from '@lucide/svelte'
  import Tree from './Tree.svelte'
  import { app, move, join, parent, title, exists, flash, persist, type Node } from './store.svelte'

  let { nodes, depth = 0, onmenu }: { nodes: Node[]; depth?: number; onmenu: (e: MouseEvent, n: Node | null) => void } = $props()
  let over = $state<string | null>(null)

  function click(n: Node) {
    app.selected = n.path
    if (n.dir) {
      app.open.has(n.path) ? app.open.delete(n.path) : app.open.add(n.path)
    } else {
      app.current = n.path
    }
    persist()
  }

  async function commit(n: Node, input: HTMLInputElement) {
    if (app.renaming !== n.path) return
    app.renaming = null
    const name = input.value.trim().replace(/[\\/]/g, '-')
    if (!name) return
    const to = join(parent(n.path), n.dir ? name : name + '.md')
    if (to === n.path) return
    if (exists(to)) return flash(t('« {name} » existe déjà ici', { name }))
    await move(n.path, to)
  }

  function drop(e: DragEvent, n: Node) {
    e.preventDefault()
    e.stopPropagation()
    over = null
    const from = e.dataTransfer?.getData('text/x-traym')
    if (!from) return
    const dir = n.dir ? n.path : parent(n.path)
    const to = join(dir, from.split('/').pop()!)
    if (to !== from && !(dir + '/').startsWith(from + '/')) {
      if (exists(to)) return flash(t('Un élément porte déjà ce nom ici'))
      app.open.add(dir)
      move(from, to)
    }
  }

  const autofocus = (el: HTMLInputElement) => {
    el.focus()
    el.select()
  }
</script>

{#each nodes as n (n.path)}
  <div
    class="row"
    class:active={app.current === n.path}
    class:selected={app.selected === n.path}
    class:over={over === n.path}
    class:pulse={app.pulse.has(n.path)}
    style:--depth={depth}
    role="treeitem"
    aria-selected={app.current === n.path}
    aria-expanded={n.dir ? app.open.has(n.path) : undefined}
    tabindex="-1"
    draggable={app.renaming !== n.path}
    onclick={() => click(n)}
    onkeydown={(e) => e.key === 'Enter' && click(n)}
    oncontextmenu={(e) => onmenu(e, n)}
    ondblclick={() => !n.dir && (app.renaming = n.path)}
    ondragstart={(e) => e.dataTransfer?.setData('text/x-traym', n.path)}
    ondragover={(e) => { e.preventDefault(); e.stopPropagation(); over = n.dir ? n.path : null }}
    ondragleave={() => (over = null)}
    ondrop={(e) => drop(e, n)}
  >
    {#if n.dir}
      <span class="chev" class:open={app.open.has(n.path)}><ChevronRight size={13} strokeWidth={1.75} /></span>
      <span class="glyph">
        {#if app.open.has(n.path)}<FolderOpen size={14} strokeWidth={1.5} />{:else}<Folder size={14} strokeWidth={1.5} />{/if}
      </span>
    {:else}
      <span class="chev"></span>
    {/if}
    {#if app.renaming === n.path}
      <input
        class="rename"
        value={n.dir ? n.name : title(n.path)}
        use:autofocus
        spellcheck="false"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => {
          e.stopPropagation()
          if (e.key === 'Enter') e.currentTarget.blur()
          if (e.key === 'Escape') app.renaming = null
        }}
        onblur={(e) => commit(n, e.currentTarget)}
      />
    {:else}
      <span class="name">{n.dir ? n.name : title(n.path)}</span>
    {/if}
  </div>
  {#if n.dir && app.open.has(n.path)}
    <div class="group" role="group">
      <Tree nodes={n.children} depth={depth + 1} {onmenu} />
    </div>
  {/if}
{/each}

<style>
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    height: 28px;
    padding: 0 8px 0 calc(6px + var(--depth) * 14px);
    margin: 0 6px;
    border-radius: var(--r);
    color: var(--muted);
    white-space: nowrap;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .row:hover { background: var(--hover); color: var(--fg); }
  /* quiet selection: a neutral fill, ink one step darker, no color */
  .row.active, .row.active:hover {
    background: color-mix(in srgb, var(--fg) 8%, transparent);
    color: var(--fg);
  }
  .row.active .name { font-weight: 600; }
  .row.selected:not(.active) { box-shadow: inset 0 0 0 1px var(--line); color: var(--fg); }
  .row.over { background: var(--orange-wash); box-shadow: inset 0 0 0 1px var(--orange); }
  .row.pulse { animation: pulse 1.6s var(--ease); }
  @keyframes pulse {
    0%, 40% { color: var(--orange); background: var(--orange-wash); }
    100% { background: transparent; }
  }

  .chev {
    display: grid;
    place-items: center;
    width: 14px;
    flex: none;
    color: var(--faint);
    transition: transform var(--t) var(--ease);
  }
  .chev.open { transform: rotate(90deg); }
  .glyph { display: grid; color: var(--faint); margin-right: 3px; }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
  }
  .rename {
    flex: 1;
    min-width: 0;
    height: 22px;
    padding: 0 6px;
    margin-left: -6px;
    border: 1px solid var(--orange);
    border-radius: 4px;
    background: var(--bg);
    font-weight: 500;
  }
  .group { animation: open var(--t) var(--ease); }
  @keyframes open {
    from { opacity: 0; transform: translateY(-3px); }
  }
</style>
