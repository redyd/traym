<script lang="ts">
  import { onMount } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { open } from '@tauri-apps/plugin-dialog'
  import { documentDir, join } from '@tauri-apps/api/path'
  import { fly } from 'svelte/transition'
  import { PanelLeft, Sparkles, Sun, Moon, Minus, Square, X, CircleAlert } from '@lucide/svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import Editor from './lib/Editor.svelte'
  import AgentPanel from './lib/AgentPanel.svelte'
  import Palette from './lib/Palette.svelte'
  import { app, openVault, newNote, newFolder, persist, parent, title, mod } from './lib/store.svelte'

  const win = getCurrentWindow()
  let dark = $state(true)
  let anim = $state(false)
  let ready = $state(false)

  const AGENT_W = 368

  function applyTheme() {
    const sys = matchMedia('(prefers-color-scheme: dark)').matches
    dark = app.theme === 'system' ? sys : app.theme === 'dark'
    document.documentElement.dataset.theme = dark ? 'dark' : 'light'
  }

  function animate(fn: () => void) {
    anim = true
    fn()
    persist()
    setTimeout(() => (anim = false), 260)
  }
  const toggleSidebar = () => animate(() => (app.sidebar = !app.sidebar))
  const toggleAgent = () => animate(() => (app.agent = !app.agent))
  function toggleTheme() {
    app.theme = dark ? 'light' : 'dark'
    applyTheme()
    persist()
  }

  async function chooseVault() {
    const dir = await open({ directory: true, title: 'Choisir le dossier de notes' })
    if (typeof dir === 'string') await openVault(dir)
  }

  const commands = [
    { label: 'Nouvelle note', keys: [mod, 'N'], run: () => newNote() },
    { label: 'Nouveau dossier', run: () => newFolder(app.current ? parent(app.current) : '') },
    { label: 'Ouvrir une note', keys: [mod, 'P'], run: () => setTimeout(() => (app.palette = 'files')) },
    { label: 'Afficher ou masquer la barre latérale', keys: [mod, '\\'], run: toggleSidebar },
    { label: "Afficher ou masquer l'agent", keys: [mod, 'J'], run: toggleAgent },
    { label: 'Changer de thème', keys: [mod, 'Maj', 'L'], run: toggleTheme },
    { label: 'Suivre le thème du système', run: () => { app.theme = 'system'; applyTheme(); persist() } },
    { label: 'Changer de dossier de notes', run: chooseVault },
  ]

  function keys(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || !app.vault) return
    const k = e.key.toLowerCase()
    const map: Record<string, () => void> = {
      p: () => (app.palette = app.palette === 'files' ? null : 'files'),
      k: () => (app.palette = app.palette === 'commands' ? null : 'commands'),
      n: () => newNote(),
      '\\': toggleSidebar,
      j: toggleAgent,
    }
    const fn = e.shiftKey ? (k === 'l' ? toggleTheme : undefined) : map[k]
    if (fn) {
      e.preventDefault()
      fn()
    }
  }

  onMount(() => {
    applyTheme()
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme)
    // first launch: no picker, straight into ~/Documents/traym
    ;(async () => app.vault ?? join(await documentDir(), 'traym'))()
      .then(openVault)
      .catch(() => (app.vault = null))
      .finally(() => (ready = true))
  })

  const crumbs = $derived(app.current ? [...parent(app.current).split('/').filter(Boolean), title(app.current)] : [])
</script>

<svelte:window onkeydown={keys} />

<div class="shell">
  <header class="titlebar" data-tauri-drag-region>
    <div class="left" data-tauri-drag-region>
      <button class="mark" title="Ouvrir un autre dossier" onclick={chooseVault}><i></i>traym</button>
      {#if app.vault}
        <button class="icon-btn" aria-pressed={app.sidebar} title="Barre latérale ({mod}+\)" onclick={toggleSidebar}>
          <PanelLeft size={15} strokeWidth={1.5} />
        </button>
      {/if}
    </div>

    <div class="crumbs" data-tauri-drag-region>
      {#each crumbs as c, i}
        {#if i}<span class="sep" data-tauri-drag-region>/</span>{/if}
        <span class:leaf={i === crumbs.length - 1} data-tauri-drag-region>{c}</span>
      {/each}
    </div>

    <div class="right" data-tauri-drag-region>
      <button class="icon-btn" title="Thème ({mod}+Maj+L)" onclick={toggleTheme}>
        {#if dark}<Sun size={15} strokeWidth={1.5} />{:else}<Moon size={15} strokeWidth={1.5} />{/if}
      </button>
      {#if app.vault}
        <button class="icon-btn" aria-pressed={app.agent} title="Agent ({mod}+J)" onclick={toggleAgent}>
          <Sparkles size={15} strokeWidth={1.5} />
        </button>
      {/if}
      <span class="divider"></span>
      <button class="icon-btn win" title="Réduire" onclick={() => win.minimize()}><Minus size={14} strokeWidth={1.5} /></button>
      <button class="icon-btn win" title="Agrandir" onclick={() => win.toggleMaximize()}><Square size={11} strokeWidth={1.6} /></button>
      <button class="icon-btn win close" title="Fermer" onclick={() => win.close()}><X size={15} strokeWidth={1.5} /></button>
    </div>
  </header>

  {#if !ready}
    <div class="body"></div>
  {:else if app.vault}
    <main
      class="body grid"
      class:anim
      style:grid-template-columns="{app.sidebar ? app.sidebarW : 0}px minmax(0, 1fr) {app.agent ? AGENT_W : 0}px"
    >
      <div class="pane" inert={!app.sidebar}>
        <div style:width="{app.sidebarW}px" style:height="100%"><Sidebar /></div>
      </div>
      <Editor />
      <div class="pane right" inert={!app.agent}>
        <div style:width="{AGENT_W}px" style:height="100%"><AgentPanel /></div>
      </div>
    </main>
  {:else}
    <main class="body welcome">
      <h1><i></i>traym</h1>
      <p>Vos notes en markdown, dans un dossier de votre disque. Claude et Codex peuvent les lire et les ranger avec vous.</p>
      <button onclick={chooseVault}>Choisir un dossier de notes</button>
    </main>
  {/if}

  {#if app.palette}<Palette {commands} />{/if}

  {#if app.error}
    <div class="toast" role="alert" transition:fly={{ y: 10, duration: 220, opacity: 0 }}>
      <CircleAlert size={15} strokeWidth={1.75} />
      <span>{app.error}</span>
      <button onclick={() => (app.error = null)} aria-label="Fermer"><X size={13} strokeWidth={1.75} /></button>
    </div>
  {/if}
</div>

<style>
  .shell { height: 100%; display: flex; flex-direction: column; overflow: hidden; }

  .titlebar {
    flex: none;
    height: 40px;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    padding: 0 6px 0 14px;
    background: var(--bg-deep);
    border-bottom: 1px solid var(--line-soft);
  }
  .left, .right { display: flex; align-items: center; gap: 2px; }
  .right { justify-content: flex-end; }
  .mark {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-right: 10px;
    font: 700 14px/1 var(--display);
    letter-spacing: -0.03em;
    color: var(--fg);
    padding: 4px 6px;
    margin-left: -6px;
    border-radius: var(--r);
  }
  .mark:hover { background: var(--orange-wash); }
  .mark i, .welcome h1 i {
    display: block;
    width: 2px;
    height: 14px;
    background: var(--orange);
    border-radius: 1px;
  }
  .crumbs {
    display: flex;
    gap: 6px;
    min-width: 0;
    color: var(--faint);
    font-weight: 500;
    white-space: nowrap;
  }
  .crumbs .leaf { color: var(--muted); }
  .sep { opacity: 0.6; }
  .divider { width: 1px; height: 16px; background: var(--line-soft); margin: 0 6px; }
  .win { width: 30px; }
  .close:hover { background: var(--orange); color: #fff; }

  .body { flex: 1; min-height: 0; }
  .grid { display: grid; grid-template-rows: minmax(0, 1fr); overflow: hidden; }
  .grid.anim { transition: grid-template-columns 240ms var(--ease); }
  .pane { overflow: hidden; height: 100%; }
  .pane.right { display: flex; justify-content: flex-end; }

  .welcome {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    max-width: 520px;
    margin: 0 auto;
    padding: 0 32px 10vh;
  }
  .welcome h1 {
    display: flex;
    align-items: center;
    gap: 14px;
    margin: 0 0 20px -2px;
    font: 750 76px/1 var(--display);
    letter-spacing: -0.055em;
  }
  .welcome h1 i {
    width: 5px;
    height: 64px;
    border-radius: 2px;
    animation: caret 1.1s steps(1) infinite;
  }
  @keyframes caret { 50% { opacity: 0; } }
  .welcome p {
    margin: 0 0 28px;
    font: 16px/1.6 var(--text);
    color: var(--muted);
  }
  .welcome button {
    padding: 10px 16px;
    border-radius: var(--r);
    background: var(--orange);
    color: #fff;
    font-weight: 650;
    font-size: 13.5px;
    transition: transform var(--t-fast) var(--ease), filter var(--t-fast) var(--ease);
  }
  .welcome button:hover { filter: brightness(1.06); }
  .welcome button:active { transform: scale(.98); }

  .toast {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 20px;
    width: fit-content;
    margin: 0 auto; /* centred without transform, so the fly transition can own it */
    z-index: 60;
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: min(520px, calc(100vw - 32px));
    padding: 10px 10px 10px 14px;
    background: var(--surface-solid);
    border: 1px solid var(--line-soft);
    border-radius: 10px;
    box-shadow: 0 1px 0 #ffffff08 inset, 0 16px 40px -16px #00000080, 0 2px 6px -2px #00000040;
    font: 500 13px/1.4 var(--text);
    color: var(--fg);
  }
  .toast > :global(svg) { flex: none; color: var(--orange); }
  .toast span { flex: 1; min-width: 0; }
  .toast button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-left: 4px;
    border-radius: 6px;
    color: var(--faint);
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .toast button:hover { background: var(--hover); color: var(--fg); }
</style>
