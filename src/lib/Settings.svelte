<script lang="ts">
  import { onMount } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { toast } from 'svelte-sonner'
  import { X, Plus, Trash2, Copy, Eye, EyeOff, RefreshCw, Check, CircleAlert } from '@lucide/svelte'
  import { app, persist, backend, loadSettings, saveSettings, ACCENTS, type Provider } from './store.svelte'
  import { i18n, t, type Lang } from './i18n.svelte'

  let { onclose }: { onclose: () => void } = $props()

  type Section = 'general' | 'agents' | 'mcp'
  let section = $state<Section>('general')
  const sections: [Section, string][] = [['general', 'Général'], ['agents', 'Agents IA'], ['mcp', 'MCP']]

  let clis = $state<Record<string, boolean>>({})
  let tests = $state<Record<string, { state: 'busy' | 'ok' | 'fail'; text: string }>>({})
  let showToken = $state(false)

  onMount(() => {
    loadSettings().catch(() => {})
    invoke<Record<string, boolean>>('agent_available').then((f) => (clis = f), () => {})
  })

  // ---- general: applied on change, kept in localStorage by persist()
  function setLang(v: Lang) {
    i18n.lang = v
    persist()
  }
  function setTheme(v: typeof app.theme) {
    app.theme = v
    persist()
  }
  // every General option is one field of `app`, applied live and kept by persist()
  function set<K extends 'density' | 'accent' | 'toolbarAlign' | 'pageAlign' | 'onClose'>(k: K, v: (typeof app)[K]) {
    app[k] = v
    persist()
  }
  const setDensity = (v: typeof app.density) => set('density', v)
  const accentNames: Record<string, string> = { orange: 'Orange', blue: 'Bleu', green: 'Vert', violet: 'Violet', pink: 'Rose', red: 'Rouge' }
  const custom = $derived(!(app.accent in ACCENTS))
  const isDark = $derived(app.theme === 'dark' || (app.theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches))

  // ---- custom providers: saved on every committed change
  const presets = [
    { name: 'Ollama', baseUrl: 'http://localhost:11434/v1' },
    { name: 'LM Studio', baseUrl: 'http://localhost:1234/v1' },
    { name: 'Autre API', baseUrl: 'https://' },
  ]
  const save = (providers: Provider[]) => saveSettings({ ...backend.settings, providers }).catch(() => {})

  function add(p: { name: string; baseUrl: string }) {
    const id = crypto.randomUUID().slice(0, 8)
    save([...backend.settings.providers, { id, name: t(p.name), baseUrl: p.baseUrl, apiKey: '' }])
  }
  function update(id: string, patch: Partial<Provider>) {
    delete tests[id]
    save(backend.settings.providers.map((p) => (p.id === id ? { ...p, ...patch } : p)))
  }
  const remove = (id: string) => save(backend.settings.providers.filter((p) => p.id !== id))

  async function test(id: string) {
    tests[id] = { state: 'busy', text: t('Test en cours…') }
    try {
      const m = await invoke<unknown[]>('agent_models', { agent: `custom:${id}` })
      tests[id] = { state: 'ok', text: t('{n} modèles disponibles', { n: m.length }) }
    } catch (e) {
      tests[id] = { state: 'fail', text: String(e) }
    }
  }

  // ---- MCP
  const setExternal = (on: boolean) => saveSettings({ ...backend.settings, mcpExternal: on }).catch(() => {})
  const regenerate = () => saveSettings({ ...backend.settings, mcpToken: '' }).catch(() => {})

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text)
      toast.success(t('Copié'))
    } catch (e) {
      toast.error(String(e))
    }
  }

  const token = $derived(backend.settings.mcpToken)
  const shown = $derived(showToken ? token : '•'.repeat(24))
  // ready-to-paste setup per client; `k` is the token to put in (masked on screen, real when copied)
  const clients = (url: string, k: string) => [
    { name: 'Claude Code', where: '', text: `claude mcp add --transport http traym ${url} --header "Authorization: Bearer ${k}"` },
    { name: 'Codex', where: '~/.codex/config.toml', text: `[mcp_servers.traym]\nurl = "${url}"\nhttp_headers = { Authorization = "Bearer ${k}" }` },
    { name: 'Cursor', where: '~/.cursor/mcp.json', text: JSON.stringify({ mcpServers: { traym: { url, headers: { Authorization: `Bearer ${k}` } } } }, null, 2) },
    {
      name: 'Claude Desktop',
      where: 'claude_desktop_config.json',
      text: JSON.stringify({ mcpServers: { traym: { command: 'npx', args: ['-y', 'mcp-remote', url, '--header', `Authorization:Bearer ${k}`] } } }, null, 2),
    },
  ]
</script>

{#snippet choice<T extends string>(name: string, value: T, options: [T, string][], set: (v: T) => void)}
  <div class="seg" role="radiogroup" aria-label={name}>
    {#each options as [v, l]}
      <button role="radio" aria-checked={value === v} onclick={() => set(v)}>{t(l)}</button>
    {/each}
  </div>
{/snippet}

<main class="settings">
  <nav aria-label={t('Réglages')}>
    <h1>{t('Réglages')}</h1>
    {#each sections as [k, l]}
      <button aria-current={section === k ? 'page' : undefined} onclick={() => (section = k)}>{t(l)}</button>
    {/each}
  </nav>

  <div class="content">
    <button class="icon-btn close" aria-label={t('Fermer')} onclick={onclose}><X size={16} strokeWidth={1.5} /></button>

    {#if section === 'general'}
      <h2>{t('Général')}</h2>
      <div class="field">
        <span class="label">{t('Langue')}</span>
        {@render choice(t('Langue'), i18n.lang, [['system', 'Système'], ['fr', 'Français'], ['en', 'English']], setLang)}
      </div>
      <div class="field">
        <span class="label">{t('Thème par défaut')}</span>
        {@render choice(t('Thème par défaut'), app.theme, [['system', 'Système'], ['light', 'Clair'], ['dark', 'Sombre']], setTheme)}
      </div>
      <div class="field">
        <span class="label">{t('Densité')}<small>{t("Taille du texte et espacement de l'interface et des notes.")}</small></span>
        {@render choice(t('Densité'), app.density, [['compact', 'Compacte'], ['normal', 'Normale'], ['large', 'Large']], setDensity)}
      </div>
      <div class="field">
        <span class="label">{t("Couleur d'accent")}</span>
        <div class="swatches" role="radiogroup" aria-label={t("Couleur d'accent")}>
          {#each Object.entries(ACCENTS) as [k, c]}
            <button role="radio" aria-checked={app.accent === k} aria-label={t(accentNames[k])} title={t(accentNames[k])} style:--c={isDark ? c.dark : c.light} onclick={() => set('accent', k)}></button>
          {/each}
          <label class="custom" class:on={custom} title={t('Couleur personnalisée')} style:--c={custom ? app.accent : undefined}>
            <input type="color" aria-label={t('Couleur personnalisée')} value={custom ? app.accent : '#888888'} oninput={(e) => set('accent', e.currentTarget.value)} />
          </label>
        </div>
      </div>
      <div class="field">
        <span class="label">{t("Barre d'outils")}</span>
        {@render choice(t("Barre d'outils"), app.toolbarAlign, [['center', 'Centrée'], ['left', 'À gauche']], (v) => set('toolbarAlign', v))}
      </div>
      <div class="field">
        <span class="label">{t('Page')}<small>{t('Position du texte des notes dans la fenêtre.')}</small></span>
        {@render choice(t('Page'), app.pageAlign, [['center', 'Centrée'], ['left', 'À gauche']], (v) => set('pageAlign', v))}
      </div>
      <div class="field">
        <span class="label">{t('À la fermeture')}<small>{t('Avec un dépôt git distant. Sans, traym enregistre simplement dans git.')}</small></span>
        {@render choice(t('À la fermeture'), app.onClose, [['ask', 'Demander'], ['save', 'Sauvegarder'], ['sync', 'Sauvegarder & synchroniser']], (v) => set('onClose', v))}
      </div>
    {:else if section === 'agents'}
      <h2>{t('Agents IA')}</h2>
      <h3>{t('Agents en ligne de commande')}</h3>
      <ul class="clis">
        {#each [['claude', 'Claude Code'], ['codex', 'Codex']] as [k, name]}
          <li>
            <span>{name}</span><code>{k}</code>
            {#if clis[k] === undefined}<span class="status">…</span>
            {:else if clis[k]}<span class="status ok"><Check size={13} strokeWidth={2} />{t('Détecté')}</span>
            {:else}<span class="status">{t('Non installé')}</span>{/if}
          </li>
        {/each}
      </ul>

      <h3>{t('IA personnalisées')}</h3>
      <p class="hint">{t("Toute API compatible OpenAI : Ollama, LM Studio, OpenRouter, Mistral, Groq… traym lui donne les mêmes outils de notes qu'à Claude et Codex.")}</p>
      {#each backend.settings.providers as p (p.id)}
        {@const r = tests[p.id]}
        <section class="provider">
          <div class="grid">
            <label>
              <span>{t('Nom')}</span>
              <input value={p.name} spellcheck="false" onchange={(e) => update(p.id, { name: e.currentTarget.value.trim() || p.name })} />
            </label>
            <label>
              <span>{t('URL de base')}</span>
              <input value={p.baseUrl} spellcheck="false" autocomplete="off" onchange={(e) => update(p.id, { baseUrl: e.currentTarget.value.trim() })} />
            </label>
            <label class="wide">
              <span>{t('Clé API (facultative)')}</span>
              <input type="password" value={p.apiKey} autocomplete="off" onchange={(e) => update(p.id, { apiKey: e.currentTarget.value.trim() })} />
            </label>
          </div>
          <div class="row">
            <button class="btn" disabled={r?.state === 'busy'} onclick={() => test(p.id)}>{t('Tester')}</button>
            {#if r}
              <span class="result {r.state}" title={r.text}>
                {#if r.state === 'ok'}<Check size={13} strokeWidth={2} />{:else if r.state === 'fail'}<CircleAlert size={13} strokeWidth={2} />{/if}{r.text}
              </span>
            {/if}
            <button class="btn danger" onclick={() => remove(p.id)}><Trash2 size={13} strokeWidth={1.75} />{t('Supprimer')}</button>
          </div>
        </section>
      {/each}
      <div class="add">
        <Plus size={14} strokeWidth={1.75} /><span>{t('Ajouter')}</span>
        {#each presets as pr}
          <button class="btn" onclick={() => add(pr)}>{t(pr.name)}</button>
        {/each}
      </div>
    {:else}
      <h2>{t('Serveur MCP')}</h2>
      <p class="hint">{t("traym expose vos notes en MCP. Activez l'accès externe pour que d'autres agents (Claude Code, Codex, Cursor, Claude Desktop…) puissent les lire et les modifier, avec le jeton ci-dessous.")}</p>
      <label class="switch">
        <input type="checkbox" role="switch" checked={backend.settings.mcpExternal} onchange={(e) => setExternal(e.currentTarget.checked)} />
        <span class="track" aria-hidden="true"></span>
        {t('Autoriser les agents externes')}
      </label>

      {#if backend.settings.mcpExternal && token}
        <div class="field stack">
          <span class="label">{t('URL du serveur')}</span>
          <div class="value">
            <code>{backend.mcpUrl}</code>
            <button class="icon-btn" aria-label={t('Copier')} onclick={() => copy(backend.mcpUrl)}><Copy size={14} strokeWidth={1.5} /></button>
          </div>
          <small>{t("L'URL n'est valable que pendant que traym est ouvert. Le port peut changer si 7340 est déjà pris.")}</small>
        </div>
        <div class="field stack">
          <span class="label">{t('Jeton')}</span>
          <div class="value">
            <code>{shown}</code>
            <button class="icon-btn" aria-label={t(showToken ? 'Masquer' : 'Afficher')} onclick={() => (showToken = !showToken)}>
              {#if showToken}<EyeOff size={14} strokeWidth={1.5} />{:else}<Eye size={14} strokeWidth={1.5} />{/if}
            </button>
            <button class="icon-btn" aria-label={t('Copier')} onclick={() => copy(token)}><Copy size={14} strokeWidth={1.5} /></button>
            <button class="icon-btn" aria-label={t('Régénérer')} title={t('Régénérer le jeton déconnecte les agents qui utilisent l’ancien.')} onclick={regenerate}><RefreshCw size={14} strokeWidth={1.5} /></button>
          </div>
        </div>
        {#each clients(backend.mcpUrl, shown) as c, i}
          <div class="snippet">
            <div class="head">
              <span>{t('Configuration pour {client}', { client: c.name })}</span>
              {#if c.where}<code>{c.where}</code>{/if}
              <button class="icon-btn" aria-label={t('Copier')} onclick={() => copy(clients(backend.mcpUrl, token)[i].text)}><Copy size={14} strokeWidth={1.5} /></button>
            </div>
            <pre>{c.text}</pre>
          </div>
        {/each}
      {/if}
    {/if}
  </div>
</main>

<style>
  .settings {
    position: fixed;
    inset: 40px 0 0; /* below the titlebar */
    z-index: 20;
    animation: settings-in var(--t) var(--ease);
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr);
    background: var(--bg);
  }
  @keyframes settings-in { from { opacity: 0; } }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 20px 10px;
    background: var(--bg-deep);
    border-right: 1px solid var(--line-soft);
  }
  nav h1 { margin: 0 10px 14px; font: 650 15px/1.2 var(--display); }
  nav button {
    text-align: left;
    padding: 7px 10px;
    border-radius: var(--r);
    color: var(--muted);
    font-weight: 500;
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  nav button:hover { background: var(--hover); color: var(--fg); }
  nav button[aria-current='page'] { background: var(--surface); color: var(--fg); }

  .content { position: relative; overflow-y: auto; padding: 28px 40px 48px; max-width: 760px; user-select: text; }
  .close { position: absolute; top: 16px; right: 16px; }
  h2 { margin: 0 0 22px; font: 650 20px/1.3 var(--display); }
  h3 { margin: 28px 0 8px; font: 600 13px/1.3 var(--display); color: var(--fg); }
  .hint { margin: 0 0 14px; color: var(--muted); line-height: 1.55; }
  small { display: block; margin-top: 3px; color: var(--faint); font-size: 12px; font-weight: 400; line-height: 1.45; }

  .field { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 14px 0; border-bottom: 1px solid var(--line-soft); }
  .field.stack { flex-direction: column; align-items: stretch; gap: 6px; border: 0; padding: 10px 0; }
  .label { font-weight: 600; }

  .seg { display: flex; flex: none; padding: 2px; border: 1px solid var(--line-soft); border-radius: var(--r); }
  .seg button { padding: 5px 12px; border-radius: 4px; color: var(--faint); font-weight: 500; transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease); }
  .seg button:hover { color: var(--fg); }
  .seg button[aria-checked='true'] { background: var(--surface); color: var(--fg); }

  .swatches { display: flex; align-items: center; gap: 8px; }
  .swatches button, .custom {
    position: relative;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--fg) 12%, transparent);
    transition: transform var(--t-fast) var(--ease), box-shadow var(--t-fast) var(--ease);
  }
  .swatches button:hover, .custom:hover { transform: scale(1.1); }
  .swatches [aria-checked='true'], .custom.on { box-shadow: 0 0 0 2px var(--bg), 0 0 0 4px var(--c); }
  .custom { cursor: pointer; background: var(--c, conic-gradient(#f87171, #fbbf24, #4cc38a, #5b9dff, #a78bfa, #f472b6, #f87171)); }
  .custom input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .custom:focus-within { outline: 1.5px solid var(--orange); outline-offset: 4px; }

  .clis { list-style: none; margin: 0; padding: 0; }
  .clis li { display: flex; align-items: center; gap: 8px; padding: 8px 0; border-bottom: 1px solid var(--line-soft); }
  .clis code, .value code, .head code { font: 12px var(--mono); color: var(--faint); }
  .status { margin-left: auto; display: flex; align-items: center; gap: 4px; color: var(--faint); font-size: 12px; }
  .status.ok { color: var(--fg); }
  .status.ok :global(svg) { color: var(--orange); }

  .provider { padding: 14px; margin-bottom: 10px; border: 1px solid var(--line-soft); border-radius: 8px; background: var(--bg-deep); }
  .grid { display: grid; grid-template-columns: 1fr 2fr; gap: 10px; }
  .grid .wide { grid-column: 1 / -1; }
  .grid label span { display: block; margin-bottom: 4px; font-size: 12px; color: var(--muted); }
  input:not([type='checkbox']) {
    width: 100%;
    padding: 6px 9px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--bg);
    font: 12.5px var(--mono);
    transition: border-color var(--t-fast) var(--ease);
  }
  input:focus-visible { outline: none; border-color: var(--orange); }
  .row { display: flex; align-items: center; gap: 10px; margin-top: 12px; min-width: 0; }
  .result { display: flex; align-items: center; gap: 5px; min-width: 0; font-size: 12px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .result :global(svg) { flex: none; }
  .result.ok :global(svg) { color: var(--orange); }
  .result.fail { color: var(--orange); }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    padding: 5px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    font-weight: 500;
    color: var(--muted);
    transition: border-color var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .btn:not(:disabled):hover { border-color: var(--orange); color: var(--fg); }
  .btn.danger { margin-left: auto; border-color: transparent; }
  .btn.danger:hover { color: var(--orange); border-color: transparent; }
  .add { display: flex; align-items: center; gap: 8px; margin-top: 12px; color: var(--muted); flex-wrap: wrap; }

  .switch { display: flex; align-items: center; gap: 10px; margin: 6px 0 18px; font-weight: 600; cursor: pointer; }
  .switch input { position: absolute; opacity: 0; width: 1px; height: 1px; }
  .track { position: relative; width: 32px; height: 18px; flex: none; border-radius: 9px; background: var(--line); transition: background var(--t-fast) var(--ease); }
  .track::after { content: ''; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: var(--fg); transition: transform var(--t-fast) var(--ease); }
  .switch input:checked + .track { background: var(--orange); }
  .switch input:checked + .track::after { transform: translateX(14px); background: var(--bg); }
  .switch input:focus-visible + .track { outline: 1.5px solid var(--orange); outline-offset: 2px; }

  .value { display: flex; align-items: center; gap: 4px; padding: 4px 4px 4px 10px; border: 1px solid var(--line-soft); border-radius: var(--r); background: var(--bg-deep); }
  .value code { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg); }
  .snippet { margin-top: 14px; border: 1px solid var(--line-soft); border-radius: 8px; overflow: hidden; }
  .head { display: flex; align-items: center; gap: 8px; padding: 6px 6px 6px 12px; background: var(--bg-deep); border-bottom: 1px solid var(--line-soft); font-weight: 600; }
  .head button { margin-left: auto; }
  pre { margin: 0; padding: 10px 12px; white-space: pre-wrap; overflow-wrap: anywhere; font: 12px/1.55 var(--mono); color: var(--muted); }
</style>
