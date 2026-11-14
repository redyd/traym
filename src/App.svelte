<script lang="ts">
  import { onMount } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { open } from '@tauri-apps/plugin-dialog'
  import { documentDir, join } from '@tauri-apps/api/path'
  import { Tooltip, AlertDialog, Checkbox } from 'bits-ui'
  import { Toaster } from 'svelte-sonner'
  import { PanelLeft, Sparkles, Sun, Moon, Minus, Square, X, Settings, Check, CloudUpload, GitCommitHorizontal, TriangleAlert } from '@lucide/svelte'
  import Tip from './lib/Tip.svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import Editor from './lib/Editor.svelte'
  import AgentPanel from './lib/AgentPanel.svelte'
  import { t, lang } from './lib/i18n.svelte'
  import { app, openVault, newNote, newFolder, persist, parent, title, mod, quit, leave, loadSettings, checkUpdate, ACCENTS } from './lib/store.svelte'

  const win = getCurrentWindow()
  let dark = $state(true)
  let anim = $state(false)
  let ready = $state(false)

  const EDITOR_MIN = 440 // below this the editor is unusable: panels float over it instead
  let winW = $state(innerWidth)
  // each panel stays in the layout only if the editor keeps its room (the agent first); otherwise it floats
  const agentDocked = $derived(app.agent && winW >= app.agentW + EDITOR_MIN)
  const sideDocked = $derived(app.sidebar && winW >= app.sidebarW + (agentDocked ? app.agentW : 0) + EDITOR_MIN)
  const floating = $derived((app.sidebar && !sideDocked) || (app.agent && !agentDocked))
  function closeFloating() {
    if (!sideDocked) app.sidebar = false
    if (!agentDocked) app.agent = false
    persist()
  }

  // drag a panel's inner edge; the width is kept between sessions
  function resize(e: PointerEvent, side: 'left' | 'right') {
    const el = e.currentTarget as HTMLElement
    el.setPointerCapture(e.pointerId)
    const move = (ev: PointerEvent) => {
      if (side === 'left') app.sidebarW = Math.max(180, Math.min(420, ev.clientX))
      else app.agentW = Math.max(300, Math.min(640, innerWidth - ev.clientX))
    }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', () => { el.removeEventListener('pointermove', move); persist() }, { once: true })
  }

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
    if (e.key === 'Escape' && app.closing === 'ask') return void (app.closing = null)
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
      .finally(() => {
        ready = true
        // index.html's neutral boot screen covers the start: fade it out once the first real frame is painted
        requestAnimationFrame(() => requestAnimationFrame(() => {
          const boot = document.getElementById('boot')
          boot?.classList.add('gone')
          setTimeout(() => boot?.remove(), 300)
        }))
        checkUpdate()
      })
  })

  // density and language apply to the whole document (CSS variables, lang attribute)
  $effect(() => {
    app.theme // set from Settings too
    applyTheme()
  })
  // accent: presets have a shade per theme, a custom colour is used as is; the washes derive from it in CSS
  $effect(() => {
    const p = ACCENTS[app.accent]
    const c = p ? p[dark ? 'dark' : 'light'] : app.accent
    if (app.accent === 'orange') document.documentElement.style.removeProperty('--orange')
    else document.documentElement.style.setProperty('--orange', c)
  })
  let remember = $state(false)
  let message = $state('') // the close popup's commit message (empty: git.rs's default)
  let msgEl = $state<HTMLInputElement>()
  $effect(() => {
    document.documentElement.dataset.toolbar = app.toolbarAlign
    document.documentElement.dataset.page = app.pageAlign
    document.documentElement.dataset.density = app.density
    document.documentElement.lang = lang()
  })

  const crumbs = $derived(app.current ? [...parent(app.current).split('/').filter(Boolean), title(app.current)] : [])
</script>

<svelte:window onkeydown={keys} bind:innerWidth={winW} />

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
    {#await import('./lib/Conflicts.svelte') then { default: Conflicts }}<Conflicts />{/await}
  {:else if app.vault}
    <main
      class="body grid"
      inert={app.settings}
      class:anim
      style:grid-template-columns="{sideDocked ? app.sidebarW : 0}px minmax(0, 1fr) {agentDocked ? app.agentW : 0}px"
    >
      <div class="pane" class:float={app.sidebar && !sideDocked} class:shown={app.sidebar} inert={!app.sidebar}>
        <div style:width="{app.sidebarW}px" style:height="100%"><Sidebar /></div>
        <div class="handle" role="separator" aria-orientation="vertical" onpointerdown={(e) => resize(e, 'left')}></div>
      </div>
      <Editor />
      <div class="pane right" class:float={app.agent && !agentDocked} class:shown={app.agent} inert={!app.agent}>
        <div style:width="{app.agentW}px" style:height="100%"><AgentPanel /></div>
        <div class="handle" role="separator" aria-orientation="vertical" onpointerdown={(e) => resize(e, 'right')}></div>
      </div>
      {#if floating}
        <button class="scrim" aria-label={t('Fermer les panneaux')} style:right="{agentDocked ? app.agentW : 0}px" onclick={closeFloating}></button>
      {/if}
    </main>
  {:else}
    <main class="body welcome">
      <h1><i></i>traym</h1>
      <p>{t('Vos notes en markdown, dans un dossier de votre disque. Claude et Codex peuvent les lire et les ranger avec vous.')}</p>
      <button onclick={chooseVault}>{t('Choisir un dossier de notes')}</button>
    </main>
  {/if}

  <!-- over the main view, which stays mounted: the chat and the open note survive a trip to Settings -->
  <!-- rarely opened: loaded on first use, so the start doesn't parse them -->
  {#if app.settings}{#await import('./lib/Settings.svelte') then { default: SettingsPage }}<SettingsPage onclose={() => (app.settings = false)} />{/await}{/if}

  {#if app.palette && !locked}{#await import('./lib/Palette.svelte') then { default: Palette }}<Palette {commands} />{/await}{/if}

  <AlertDialog.Root open={!!app.closing} onOpenChange={(o) => !o && app.closing !== 'busy' && (app.closing = null)}>
    <AlertDialog.Portal>
      <AlertDialog.Overlay class="dlg-scrim" />
      <AlertDialog.Content class="dlg" interactOutsideBehavior="close" onOpenAutoFocus={(e) => { if (msgEl) { e.preventDefault(); msgEl.focus() } }}>
        {#if app.closing === 'busy'}
          <div class="dlg-busy"><span class="spinner" aria-hidden="true"></span><AlertDialog.Title class="dlg-title">{t(app.pushing ? 'Synchronisation…' : 'Commit…')}</AlertDialog.Title></div>
        {:else}
          {@const failed = typeof app.closing === 'object' && app.closing}
          <div class="dlg-head">
            <span class="dlg-icon">
              {#if failed}<TriangleAlert size={18} strokeWidth={1.75} />{:else}<GitCommitHorizontal size={18} strokeWidth={1.75} />{/if}
            </span>
            <div class="dlg-body">
              <AlertDialog.Title class="dlg-title">{failed ? t(app.pushing ? 'La synchronisation a échoué' : 'Le commit a échoué') : t('Avant de quitter')}</AlertDialog.Title>
              <AlertDialog.Description class="dlg-desc">
                {#if failed}{t('Tes notes sont enregistrées sur le disque, mais pas dans git.')}
                {:else}{t(app.git?.changes === 1 ? '1 fichier modifié depuis le dernier commit.' : '{n} fichiers modifiés depuis le dernier commit.', { n: app.git?.changes ?? 0 })}{/if}
              </AlertDialog.Description>
              {#if !failed}
                <input class="dlg-msg" bind:this={msgEl} bind:value={message} placeholder="traym: sync" aria-label={t('Message de commit')} spellcheck="false" autocomplete="off"
                  onkeydown={(e) => e.key === 'Enter' && leave(app.git?.remote ? 'sync' : 'commit', remember, message)} />
                <Checkbox.Root class="remember" bind:checked={remember}>
                  <span class="box"><Check size={11} strokeWidth={3} /></span>{t('Se souvenir de mon choix')}
                </Checkbox.Root>
              {/if}
            </div>
          </div>
          {#if failed}<pre class="dlg-err">{failed.error}</pre>{/if}
          <div class="dlg-foot">
            {#if failed}
              <AlertDialog.Cancel class="btn">{t('Annuler')}</AlertDialog.Cancel>
              <AlertDialog.Action class="btn primary" onclick={() => win.destroy()}>{t('Quitter quand même')}</AlertDialog.Action>
            {:else}
              <AlertDialog.Action class="btn ghost" onclick={() => leave('quit', remember)}>{t('Quitter sans commit')}</AlertDialog.Action>
              <AlertDialog.Cancel class="btn">{t('Annuler')}</AlertDialog.Cancel>
              {#if app.git?.remote}
                <AlertDialog.Action class="btn" onclick={() => leave('commit', remember, message)}>{t('Commit')}</AlertDialog.Action>
                <AlertDialog.Action class="btn primary" onclick={() => leave('sync', remember, message)}><CloudUpload size={14} strokeWidth={2} />{t('Commit & sync')}</AlertDialog.Action>
              {:else}
                <AlertDialog.Action class="btn primary" onclick={() => leave('commit', remember, message)}>{t('Commit')}</AlertDialog.Action>
              {/if}
            {/if}
          </div>
        {/if}
      </AlertDialog.Content>
    </AlertDialog.Portal>
  </AlertDialog.Root>

  <Toaster theme={dark ? 'dark' : 'light'} position="bottom-center" closeButton duration={5000} />
</div>
</Tooltip.Provider>

<style>
  .shell { height: 100%; display: flex; flex-direction: column; overflow: hidden; }

  .titlebar {
    flex: none;
    height: 40px;
    display: grid;
    /* sides keep their buttons; the breadcrumb shrinks (and stays centred while there is room) */
    grid-template-columns: minmax(max-content, 1fr) minmax(0, auto) minmax(max-content, 1fr);
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
    overflow: hidden;
    padding: 0 8px;
    color: var(--faint);
    font-weight: 500;
    white-space: nowrap;
  }
  .crumbs span:not(.sep) { overflow: hidden; text-overflow: ellipsis; }
  .crumbs .leaf { color: var(--muted); flex-shrink: 0.2; }
  @media (max-width: 600px) { .crumbs > :not(.leaf) { display: none; } } /* just the note's name */
  .sep { opacity: 0.6; }
  .divider { width: 1px; height: 16px; background: var(--line-soft); margin: 0 6px; }
  .titlebar :global(.win) { width: 30px; }
  .titlebar :global(.close:hover) { background: var(--orange); color: var(--bg); }

  .body { flex: 1; min-height: 0; }
  .grid { position: relative; display: grid; grid-template-rows: minmax(0, 1fr); overflow: hidden; }
  .grid.anim { transition: grid-template-columns 240ms var(--ease); }
  .pane { position: relative; overflow: hidden; height: 100%; grid-column: 1; }
  .handle { position: absolute; top: 0; right: 0; width: 5px; height: 100%; cursor: col-resize; z-index: 3; }
  .pane.right .handle { right: auto; left: 0; }
  .handle:hover, .handle:active { background: var(--line); }
  /* fixed columns: a floating panel leaves the flow, and the editor must not slide into its column */
  .grid > :global(.editor) { grid-column: 2; }
  .pane.right { grid-column: 3; }
  .pane > div { flex: none; } /* keeps its width when the pane floats */
  .pane.right { display: flex; justify-content: flex-end; }
  /* narrow window: a panel slides over the editor instead of squeezing it */
  .pane.float {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 15;
    max-width: calc(100% - 40px);
    box-shadow: 0 0 40px rgb(0 0 0 / 0.25);
    animation: slide-l var(--t) var(--ease);
  }
  .pane.float.right { left: auto; right: 0; animation-name: slide-r; }
  /* an absolute grid item with a grid-column is placed inside that (0 px) column: use the whole grid */
  .pane.float, .pane.float.right { grid-column: auto; }
  @keyframes slide-l { from { transform: translateX(-100%); } }
  @keyframes slide-r { from { transform: translateX(100%); } }
  .scrim {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 14;
    background: rgb(0 0 0 / 0.3);
    cursor: default;
    animation: layer-in var(--t) var(--ease);
  }

  :global(.dlg-scrim) { position: fixed; inset: 0; z-index: 100; background: rgb(0 0 0 / 0.45); animation: layer-in var(--t) var(--ease); }
  :global(.dlg) {
    position: fixed;
    z-index: 101;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    width: min(540px, calc(100vw - 32px));
    padding: 20px;
    background: var(--surface-solid);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: 0 24px 60px -20px #000000a0;
    animation: dlg-in var(--t) var(--ease);
  }
  :global(.dlg:focus-visible) { outline: none; }
  @keyframes dlg-in { from { opacity: 0; transform: translateY(6px) scale(.985); } }
  .dlg-head { display: flex; gap: 14px; align-items: flex-start; }
  .dlg-icon { flex: none; display: grid; place-items: center; width: 36px; height: 36px; border-radius: 50%; background: var(--orange-wash); color: var(--orange); }
  :global(.dlg-title) { margin: 6px 0 4px; font: 650 15px/1.3 var(--display); color: var(--fg); }
  :global(.dlg-desc) { margin: 0; font: 400 13px/1.5 var(--text); color: var(--muted); }
  .dlg-err {
    margin: 14px 0 0;
    max-height: 160px;
    overflow: auto;
    padding: 10px 12px;
    border-radius: var(--r);
    background: var(--bg-deep);
    white-space: pre-wrap;
    font: 12px/1.5 var(--mono);
    color: var(--muted);
  }
  .dlg-foot { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 8px; margin-top: 20px; }
  .dlg-body { flex: 1; min-width: 0; }
  .dlg-msg {
    display: block;
    width: 100%;
    margin-top: 12px;
    padding: 7px 9px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--bg);
    color: var(--fg);
    font: 400 13px/1.3 var(--text);
  }
  .dlg-msg:focus { outline: none; border-color: var(--orange); }
  .dlg-busy { display: flex; align-items: center; gap: 12px; }
  .dlg-busy :global(.dlg-title) { margin: 0; }
  :global(.dlg .btn) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border-radius: var(--r);
    border: 1px solid var(--line);
    font: 600 13px/1.2 var(--display);
    color: var(--fg);
    white-space: nowrap;
    transition: background var(--t-fast);
  }
  :global(.dlg .btn:hover) { background: var(--hover); }
  :global(.dlg .btn.ghost) { margin-right: auto; border-color: transparent; color: var(--muted); }
  :global(.dlg .btn.ghost:hover) { color: var(--fg); }
  :global(.dlg .btn.primary) { background: var(--orange); border-color: var(--orange); color: var(--bg); }
  :global(.dlg .btn.primary:hover) { filter: brightness(1.06); }
  :global(.dlg .btn:focus-visible) { outline: 2px solid var(--orange); outline-offset: 2px; }
  :global(.dlg .remember) {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    margin-top: 12px;
    font: 500 12.5px/1.2 var(--display);
    color: var(--muted);
    cursor: pointer;
  }
  :global(.dlg .remember .box) {
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    border: 1px solid var(--line);
    flex: none;
    box-sizing: border-box;
    border-radius: 4px;
    color: transparent;
  }
  :global(.dlg .remember[data-state='checked'] .box) { background: var(--orange); border-color: var(--orange); color: var(--bg); }
  :global(.dlg .remember:focus-visible) { outline: none; }
  :global(.dlg .remember:focus-visible .box) { outline: 2px solid var(--orange); outline-offset: 2px; }
  .spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--line);
    border-top-color: var(--orange);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

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
    color: var(--bg);
    font-weight: 650;
    font-size: 13.5px;
    transition: transform var(--t-fast) var(--ease), filter var(--t-fast) var(--ease);
  }
  .welcome button:hover { filter: brightness(1.06); }
  .welcome button:active { transform: scale(.98); }

</style>
