<script lang="ts">
  import { onMount } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { open } from '@tauri-apps/plugin-dialog'
  import { documentDir, join } from '@tauri-apps/api/path'
  import { Tooltip } from 'bits-ui'
  import { Toaster } from 'svelte-sonner'
  import { PanelLeft, Sparkles, Sun, Moon, Minus, Square, X, Settings } from '@lucide/svelte'
  import Tip from './lib/Tip.svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import Editor from './lib/Editor.svelte'
  import AgentPanel from './lib/AgentPanel.svelte'
  import Palette from './lib/Palette.svelte'
  import Conflicts from './lib/Conflicts.svelte'
  import SettingsPage from './lib/Settings.svelte'
  import { t, lang } from './lib/i18n.svelte'
  import { app, openVault, newNote, newFolder, persist, parent, title, mod, quit, loadSettings } from './lib/store.svelte'

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
    const dir = await open({ directory: true, title: t('Choisir le dossier de notes') })
    if (typeof dir === 'string') await openVault(dir)
  }

  const toggleSettings = () => (app.settings = !app.settings)
  const commands = $derived([
    { label: t('Nouvelle note'), keys: [mod, 'N'], run: () => newNote() },
    { label: t('Nouveau dossier'), run: () => newFolder(app.current ? parent(app.current) : '') },
    { label: t('Ouvrir une note'), keys: [mod, 'P'], run: () => setTimeout(() => (app.palette = 'files')) },
    { label: t('Afficher ou masquer la barre latérale'), keys: [mod, '\\'], run: toggleSidebar },
    { label: t("Afficher ou masquer l'agent"), keys: [mod, 'J'], run: toggleAgent },
    { label: t('Changer de thème'), keys: [mod, t('Maj'), 'L'], run: toggleTheme },
    { label: t('Suivre le thème du système'), run: () => { app.theme = 'system'; applyTheme(); persist() } },
    { label: t('Changer de dossier de notes'), run: chooseVault },
    { label: t('Réglages'), keys: [mod, ','], run: () => (app.settings = true) },
  ])

  function keys(e: KeyboardEvent) {
    if (e.key === 'Escape' && app.settings && !app.palette) return void (app.settings = false)
    if ((e.ctrlKey || e.metaKey) && e.key === ',') return void (e.preventDefault(), toggleSettings())
    if (!(e.ctrlKey || e.metaKey) || e.altKey || !app.vault || locked) return
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

  // a pull left conflicts: the notes stay closed until they are resolved
  const locked = $derived(!!app.git?.conflicts.length)

  onMount(() => {
    win.onCloseRequested((e) => {
      e.preventDefault()
      quit()
    })
    loadSettings().catch(() => {})
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme)
    // first launch: no picker, straight into ~/Documents/traym
    ;(async () => app.vault ?? join(await documentDir(), 'traym'))()
      .then(openVault)
      .catch(() => (app.vault = null))
      .finally(() => (ready = true))
  })

  // density and language apply to the whole document (CSS variables, lang attribute)
  $effect(() => {
    app.theme // set from Settings too
    applyTheme()
  })
  $effect(() => {
    document.documentElement.dataset.density = app.density
    document.documentElement.lang = lang()
  })

  const crumbs = $derived(app.current ? [...parent(app.current).split('/').filter(Boolean), title(app.current)] : [])
</script>

<svelte:window onkeydown={keys} />

<Tooltip.Provider delayDuration={350}>
<div class="shell">
  <header class="titlebar" data-tauri-drag-region>
    <div class="left" data-tauri-drag-region>
      <Tip class="mark" label={t('Ouvrir un autre dossier')} onclick={chooseVault}><i></i>traym</Tip>
      {#if app.vault && !locked}
        <Tip class="icon-btn" aria-pressed={app.sidebar} label={t('Barre latérale ({mod}+\\)', { mod })} onclick={toggleSidebar}>
          <PanelLeft size={15} strokeWidth={1.5} />
        </Tip>
      {/if}
    </div>

    <div class="crumbs" data-tauri-drag-region>
      {#each crumbs as c, i}
        {#if i}<span class="sep" data-tauri-drag-region>/</span>{/if}
        <span class:leaf={i === crumbs.length - 1} data-tauri-drag-region>{c}</span>
      {/each}
    </div>

    <div class="right" data-tauri-drag-region>
      <Tip class="icon-btn" aria-pressed={app.settings} label={t('Réglages ({mod}+,)', { mod })} onclick={toggleSettings}>
        <Settings size={15} strokeWidth={1.5} />
      </Tip>
      <Tip class="icon-btn" label={t(dark ? 'Thème clair ({mod}+Maj+L)' : 'Thème sombre ({mod}+Maj+L)', { mod })} onclick={toggleTheme}>
        {#if dark}<Sun size={15} strokeWidth={1.5} />{:else}<Moon size={15} strokeWidth={1.5} />{/if}
      </Tip>
      {#if app.vault && !locked}
        <Tip class="icon-btn" aria-pressed={app.agent} label={t('Agent ({mod}+J)', { mod })} onclick={toggleAgent}>
          <Sparkles size={15} strokeWidth={1.5} />
        </Tip>
      {/if}
      <span class="divider"></span>
      <Tip class="icon-btn win" label={t('Réduire')} onclick={() => win.minimize()}><Minus size={14} strokeWidth={1.5} /></Tip>
      <Tip class="icon-btn win" label={t('Agrandir')} onclick={() => win.toggleMaximize()}><Square size={11} strokeWidth={1.6} /></Tip>
      <Tip class="icon-btn win close" label={t('Fermer')} onclick={() => win.close()}><X size={15} strokeWidth={1.5} /></Tip>
    </div>
  </header>

  {#if !ready}
    <div class="body"></div>
  {:else if app.vault && locked}
    <Conflicts />
  {:else if app.vault}
    <main
      class="body grid"
      inert={app.settings}
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
      <p>{t('Vos notes en markdown, dans un dossier de votre disque. Claude et Codex peuvent les lire et les ranger avec vous.')}</p>
      <button onclick={chooseVault}>{t('Choisir un dossier de notes')}</button>
    </main>
  {/if}

  <!-- over the main view, which stays mounted: the chat and the open note survive a trip to Settings -->
  {#if app.settings}<SettingsPage onclose={() => (app.settings = false)} />{/if}

  {#if app.palette && !locked}<Palette {commands} />{/if}

  {#if app.closing}
    <div class="closing" role="alertdialog" aria-modal="true" aria-labelledby="closing-title">
      <div class="card">
        {#if app.closing === 'sync'}
          <span class="spinner" aria-hidden="true"></span>
          <p id="closing-title">{t('Synchronisation…')}</p>
        {:else}
          <p id="closing-title">{t('La synchronisation a échoué')}</p>
          <pre>{app.closing.error}</pre>
          <div class="actions">
            <button onclick={() => (app.closing = null)}>{t('Annuler')}</button>
            <button class="primary" onclick={() => win.destroy()}>{t('Quitter quand même')}</button>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <Toaster theme={dark ? 'dark' : 'light'} position="bottom-center" closeButton duration={5000} />
</div>
</Tooltip.Provider>

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
  .titlebar :global(.mark) {
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
  .titlebar :global(.mark:hover) { background: var(--orange-wash); }
  .titlebar :global(.mark i), .welcome h1 i {
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
  .titlebar :global(.win) { width: 30px; }
  .titlebar :global(.close:hover) { background: var(--orange); color: #fff; }

  .body { flex: 1; min-height: 0; }
  .grid { display: grid; grid-template-rows: minmax(0, 1fr); overflow: hidden; }
  .grid.anim { transition: grid-template-columns 240ms var(--ease); }
  .pane { overflow: hidden; height: 100%; }
  .pane.right { display: flex; justify-content: flex-end; }

  .closing {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    padding: 16px;
    background: #0006;
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    max-width: 440px;
    padding: 22px 24px;
    border-radius: 10px;
    background: var(--surface-solid);
    border: 1px solid var(--line);
    font: 600 14px/1.4 var(--display);
  }
  .card p { margin: 0; }
  .card pre {
    margin: 0;
    max-height: 160px;
    overflow: auto;
    white-space: pre-wrap;
    font: 12px/1.5 var(--mono);
    color: var(--muted);
  }
  .spinner {
    width: 22px;
    height: 22px;
    border: 2px solid var(--line);
    border-top-color: var(--orange);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .actions { display: flex; gap: 8px; }
  .actions button { padding: 8px 12px; border-radius: var(--r); font-weight: 600; }
  .actions button:hover { background: var(--hover); }
  .actions .primary { background: var(--orange); color: #fff; }
  .actions .primary:hover { background: var(--orange); filter: brightness(1.06); }

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

</style>
