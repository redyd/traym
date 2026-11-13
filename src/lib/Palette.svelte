<script lang="ts">
  import { app, flat, title, parent, persist } from './store.svelte'

  type Item = { label: string; hint?: string; keys?: string[]; run: () => void }
  let { commands }: { commands: Item[] } = $props()

  let q = $state('')
  let sel = $state(0)
  let listEl = $state<HTMLDivElement>()

  // subsequence match; consecutive and word-start hits score higher
  function score(text: string, query: string) {
    if (!query) return 1
    const t = text.toLowerCase(), s = query.toLowerCase()
    let ti = 0, total = 0, streak = 0
    for (const ch of s) {
      const at = t.indexOf(ch, ti)
      if (at < 0) return 0
      streak = at === ti ? streak + 1 : 0
      total += 1 + streak * 2 + (at === 0 || ' /-_'.includes(t[at - 1]) ? 3 : 0)
      ti = at + 1
    }
    return total - t.length * 0.01
  }

  const items: Item[] = $derived(
    app.palette === 'files'
      ? flat().map((p) => ({ label: title(p), hint: parent(p), run: () => { app.current = p; persist() } }))
      : commands,
  )
  const shown = $derived(
    items
      .map((it) => ({ it, s: score(it.hint ? `${it.hint}/${it.label}` : it.label, q) + score(it.label, q) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 50)
      .map((x) => x.it),
  )

  $effect(() => {
    q
    sel = 0
  })

  $effect(() => {
    listEl?.children[sel]?.scrollIntoView({ block: 'nearest' })
  })

  function run(it?: Item) {
    if (!it) return
    app.palette = null
    it.run()
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') { e.preventDefault(); sel = Math.min(sel + 1, shown.length - 1) }
    else if (e.key === 'ArrowUp') { e.preventDefault(); sel = Math.max(sel - 1, 0) }
    else if (e.key === 'Enter') { e.preventDefault(); run(shown[sel]) }
    else if (e.key === 'Escape') { e.preventDefault(); app.palette = null }
  }

  const focus = (el: HTMLInputElement) => el.focus()
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={() => (app.palette = null)}>
  <div class="palette" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <input
      use:focus
      bind:value={q}
      onkeydown={key}
      spellcheck="false"
      placeholder={app.palette === 'files' ? 'Rechercher une note' : 'Rechercher une commande'}
    />
    <div class="list" bind:this={listEl} role="listbox">
      {#each shown as it, i}
        <button role="option" aria-selected={i === sel} class:sel={i === sel} onmousemove={() => (sel = i)} onclick={() => run(it)}>
          <span class="label">{it.label}</span>
          {#if it.hint}<span class="hint">{it.hint}</span>{/if}
          {#if it.keys}<span class="keys">{#each it.keys as k}<kbd>{k}</kbd>{/each}</span>{/if}
        </button>
      {:else}
        <p class="none">{app.palette === 'files' ? 'Aucune note ne correspond' : 'Aucune commande ne correspond'}</p>
      {/each}
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 14vh;
    background: color-mix(in srgb, var(--bg-deep) 45%, transparent);
    animation: fade 140ms var(--ease);
  }
  .palette {
    width: min(560px, calc(100vw - 32px));
    background: var(--surface-solid);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 24px 60px -20px #000000a0;
    overflow: hidden;
    animation: drop 180ms var(--ease);
  }
  @keyframes fade { from { opacity: 0; } }
  @keyframes drop { from { opacity: 0; transform: translateY(-6px) scale(.985); } }
  input {
    width: 100%;
    padding: 15px 18px;
    border: 0;
    border-bottom: 1px solid var(--line-soft);
    background: none;
    font: 500 15.5px/1.2 var(--display);
  }
  input::placeholder { color: var(--faint); }
  .list { max-height: 340px; overflow-y: auto; padding: 6px; }
  .list button {
    display: flex;
    align-items: baseline;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border-radius: var(--r);
    text-align: left;
    color: var(--muted);
  }
  .list button.sel { background: var(--orange-wash); color: var(--fg); }
  .label { font-weight: 550; font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .hint { color: var(--faint); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .keys { margin-left: auto; display: flex; gap: 3px; }
  .none { margin: 0; padding: 14px 12px; color: var(--faint); }
</style>
