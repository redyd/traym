<script lang="ts">
  import Tip from './Tip.svelte'
  import { DropdownMenu } from 'bits-ui'
  import { onMount, tick, untrack } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { invoke } from '@tauri-apps/api/core'
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'
  import { ArrowUp, Square, SquarePen, FilePen, FileText, FolderPlus, Search, Trash2, MoveRight, List, ChevronDown, Check, Folder, ClipboardPaste, X } from '@lucide/svelte'
  import { app, call, pulse, reveal, title, flat, backend, type Node } from './store.svelte'
  import { t as tr } from './i18n.svelte'

  type Agent = string // 'claude' | 'codex' | 'custom:<provider id>'
  type Model = { id: string; name: string; default: boolean; light: boolean }
  // attached to the next message: a mentioned file/folder, or a long paste kept out of the textarea
  type Att = { kind: 'file' | 'dir'; path: string } | { kind: 'text'; text: string }
  type Line = { role: 'user' | 'ai' | 'action' | 'error'; text: string; open?: boolean; path?: string | null; tool?: string; atts?: Att[] }

  const ls = (k: string) => { try { return localStorage.getItem(k) } catch { return null } }
  let agent = $state<Agent>(ls('agent') ?? 'claude')
  // chosen model per agent, with its name so the picker reads right before the lists arrive
  let chosen = $state<Record<Agent, { id: string; name: string } | null>>(JSON.parse(ls('models') ?? '{}'))
  let models = $state<Record<Agent, Model[] | string | null>>({}) // string = error
  // CLIs: installed? custom providers: answering? (true until known otherwise)
  let found = $state<Record<Agent, boolean>>({ claude: true, codex: true })
  const agents = $derived(['claude', 'codex', ...backend.settings.providers.map((p) => `custom:${p.id}`)])
  const usable = (a: Agent) => agents.includes(a) && found[a] !== false
  const none = $derived(!agents.some(usable))
  let atts = $state<Att[]>([])
  let mention = $state<{ start: number; q: string } | null>(null)
  let hi = $state(0)
  let lines = $state<Line[]>([])
  let session: string | null = null
  let running = $state(false)
  let input = $state('')
  let list: HTMLDivElement
  let box: HTMLTextAreaElement

  const nameOf = (a: Agent) =>
    a === 'claude' ? 'Claude' : a === 'codex' ? 'Codex' : (backend.settings.providers.find((p) => `custom:${p.id}` === a)?.name ?? a)
  const label = $derived(nameOf(agent))
  // the user's pick, else a light model (cheap and quick for note chores), else the CLI's own default
  const current = (a: Agent) => {
    const m = models[a]
    return chosen[a] ?? (Array.isArray(m) ? (m.find((x) => x.light) ?? m.find((x) => x.default)) : null) ?? null
  }

  function loadModels(a: Agent) {
    const custom = a.startsWith('custom:')
    if ((!custom && !found[a]) || Array.isArray(models[a])) return
    models[a] = null
    invoke<Model[]>('agent_models', { agent: a }).then(
      (m) => { models[a] = m; found[a] = true },
      (e) => { models[a] = String(e); if (custom) found[a] = false },
    )
  }

  // custom providers come and go (Settings) and may be down: ask them again when the agent menu opens
  const probe = () => agents.filter((a) => a.startsWith('custom:') && !Array.isArray(models[a])).forEach(loadModels)
  $effect(() => {
    backend.settings.providers // a provider added or edited: forget what we knew about it
    untrack(() => {
      for (const a of Object.keys(models)) if (a.startsWith('custom:')) delete models[a]
      probe()
    })
  })

  function choose(m: Model) {
    chosen[agent] = { id: m.id, name: m.name }
    try { localStorage.setItem('models', JSON.stringify(chosen)) } catch {}
  }

  // every note and folder, for @ mentions
  const everything = (nodes: Node[] = app.tree): Node[] => nodes.flatMap((n) => [n, ...(n.dir ? everything(n.children) : [])])
  const hits = $derived.by(() => {
    if (!mention) return []
    const q = mention.q.toLowerCase()
    return everything().filter((n) => n.path.toLowerCase().includes(q)).slice(0, 8)
  })

  // an @ at a word start, up to the caret, opens the list
  function seek() {
    const m = /(?:^|\s)@([^\s@]*)$/.exec(input.slice(0, box.selectionStart))
    mention = m ? { start: box.selectionStart - m[1].length - 1, q: m[1] } : null
    hi = 0
  }

  function attach(n: Node) {
    if (!mention) return
    const end = mention.start + 1 + mention.q.length
    input = input.slice(0, mention.start) + input.slice(end)
    if (!atts.some((a) => a.kind !== 'text' && a.path === n.path)) atts.push({ kind: n.dir ? 'dir' : 'file', path: n.path })
    const at = mention.start
    mention = null
    tick().then(() => { box.setSelectionRange(at, at); grow() })
  }

  const LONG = 800
  function paste(e: ClipboardEvent) {
    const t = e.clipboardData?.getData('text/plain') ?? ''
    if (t.length < LONG && t.split('\n').length < 12) return
    e.preventDefault()
    atts.push({ kind: 'text', text: t })
  }

  const lineCount = (t: string) => t.split('\n').length
  const attName = (a: Att) => (a.kind === 'text' ? tr('Texte collé · {n} lignes', { n: lineCount(a.text) }) : a.kind === 'dir' ? a.path + '/' : title(a.path))

  function context(list: Att[]) {
    let ctx = ''
    const refs = list.flatMap((a) => (a.kind === 'text' ? [] : [a.kind === 'dir' ? a.path + '/' : a.path]))
    if (refs.length) ctx += `\n\n[Mentionnés par l'utilisateur (lis-les avec les outils traym) : ${refs.join(', ')}]`
    list.flatMap((a) => (a.kind === 'text' ? [a.text] : [])).forEach((t, i) => (ctx += `\n\n<texte_colle n="${i + 1}">\n${t}\n</texte_colle>`))
    if (app.current) ctx += `\n\n[Note ouverte dans traym : ${app.current}]`
    return ctx
  }
  const last = () => lines[lines.length - 1]
  const close = () => { const l = last(); if (l?.role === 'ai') l.open = false }

  const verbs: Record<string, string> = {
    list_notes: 'a parcouru le vault',
    read_note: 'a lu',
    write_note: 'a écrit',
    create_note: 'a créé',
    edit_note: 'a modifié',
    create_folder: 'a créé le dossier',
    move_note: 'a déplacé vers',
    delete_note: 'a mis à la corbeille',
    search_notes: 'a cherché dans les notes',
  }
  const icons: Record<string, typeof FileText> = {
    list_notes: List, read_note: FileText, write_note: FilePen, create_note: FilePen, edit_note: FilePen,
    create_folder: FolderPlus, move_note: MoveRight, delete_note: Trash2, search_notes: Search,
  }

  const render = (md: string) => DOMPurify.sanitize(marked.parse(md, { async: false }) as string)

  async function scroll() {
    await tick()
    list?.scrollTo({ top: list.scrollHeight, behavior: 'smooth' })
  }

  async function send(text = input.trim()) {
    if ((!text && !atts.length) || running || none) return
    const sent = atts
    input = ''
    atts = []
    mention = null
    lines.push({ role: 'user', text, atts: sent })
    running = true
    scroll()
    try {
      await call('agent_send', { agent, prompt: text + context(sent), session, model: current(agent)?.id ?? null })
    } catch (e) {
      lines.push({ role: 'error', text: String(e) })
    }
    running = false
    close()
  }

  function reset() {
    if (running) return
    lines = []
    session = null
    box?.focus()
  }

  function pick(a: Agent) {
    if (running || a === agent || !usable(a)) return
    agent = a
    try { localStorage.setItem('agent', a) } catch {}
    loadModels(a)
    reset() // sessions don't carry across CLIs
  }

  onMount(() => {
    invoke<Record<Agent, boolean>>('agent_available').then((f) => {
      found = { ...found, ...f }
      // the saved agent is gone (uninstalled, provider removed): take the first that works
      if (!usable(agent)) {
        const next = agents.find((a) => usable(a) && !a.startsWith('custom:'))
        if (next) pick(next)
      }
      loadModels(agent) // the light default needs the list to be named
    })
    const a = listen<any>('agent://event', ({ payload: e }) => {
      const l = last()
      switch (e.type) {
        case 'session': session = e.id; break
        case 'delta':
          if (l?.role === 'ai' && l.open) l.text += e.text
          else lines.push({ role: 'ai', text: e.text, open: true })
          break
        case 'message':
          if (l?.role === 'ai' && l.open) { l.text = e.text; l.open = false }
          else lines.push({ role: 'ai', text: e.text })
          break
        case 'tool': close(); break
        case 'error': close(); lines.push({ role: 'error', text: e.text }); break
      }
      scroll()
    })
    const m = listen<{ tool: string; path: string | null }>('mcp://call', ({ payload }) => {
      close()
      lines.push({ role: 'action', text: verbs[payload.tool] ? tr(verbs[payload.tool]) : payload.tool, path: payload.path, tool: payload.tool })
      if (payload.path && !['read_note', 'list_notes', 'search_notes'].includes(payload.tool)) {
        setTimeout(() => { reveal(payload.path!); pulse(payload.path!) }, 180) // after the tree refresh
      }
      scroll()
    })
    return () => { a.then((f) => f()); m.then((f) => f()) }
  })

  $effect(() => {
    if (app.agent) box?.focus()
  })

  function keys(e: KeyboardEvent) {
    if (hits.length) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        hi = (hi + (e.key === 'ArrowDown' ? 1 : hits.length - 1)) % hits.length
        return
      }
      if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); attach(hits[hi]); return }
    }
    if (e.key === 'Escape' && mention) { e.preventDefault(); mention = null; return }
    if (e.key === 'Backspace' && !input && atts.length) { atts.pop(); return }
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); requestAnimationFrame(grow) }
  }

  function grow() {
    box.style.height = 'auto'
    box.style.height = Math.min(box.scrollHeight, 200) + 'px'
  }

  const examples = ['Résume la note ouverte', 'Range mes notes par thème dans des dossiers', 'Crée une liste de courses dans Perso']
</script>

<aside class="agent">
  <header>
    <Tip class="icon-btn" label={tr('Nouvelle conversation')} disabled={running} onclick={reset}><SquarePen size={15} strokeWidth={1.5} /></Tip>
  </header>

  <div class="lines" bind:this={list}>
    {#if !lines.length}
      <div class="intro">
        {#if none}
          <p>{tr('Aucun agent trouvé. Installe Claude Code (claude) ou Codex (codex), ou ajoute une IA dans les réglages.')}</p>
          <button onclick={() => (app.settings = true)}>{tr('Réglages')}</button>
        {:else}
          <p>{tr('{agent} lit, écrit et range vos notes directement dans ce vault.', { agent: label })}</p>
          {#each examples as ex}
            <button onclick={() => send(tr(ex))}>{tr(ex)}</button>
          {/each}
        {/if}
      </div>
    {/if}

    {#each lines as l, i (i)}
      {#if l.role === 'user'}
        <div class="user">
          {#if l.atts?.length}
            <div class="chips">
              {#each l.atts as a}<span class="chip" title={a.kind === 'text' ? a.text.slice(0, 300) : a.path}>{attName(a)}</span>{/each}
            </div>
          {/if}
          {#if l.text}{l.text}{/if}
        </div>
      {:else if l.role === 'ai'}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="ai" class:open={l.open} onclick={(e) => (e.target as HTMLElement).closest('a') && e.preventDefault()}>
          {@html render(l.text)}
        </div>
      {:else if l.role === 'action'}
        {@const Icon = icons[l.tool ?? ''] ?? FileText}
        {@const openable = l.path && flat().includes(l.path)}
        <button class="action" disabled={!openable} onclick={() => openable && (app.current = l.path!)}>
          <Icon size={13} strokeWidth={1.75} />
          <span>{l.text}</span>
          {#if l.path}<strong>{l.path.endsWith('.md') ? title(l.path) : l.path}</strong>{/if}
        </button>
      {:else}
        <div class="error">{l.text}</div>
      {/if}
    {/each}

    {#if running && !(last()?.role === 'ai' && last()?.open)}
      <div class="dots" aria-label={tr('{agent} réfléchit', { agent: label })}><i></i><i></i><i></i></div>
    {/if}
  </div>

  <form class="composer" onsubmit={(e) => { e.preventDefault(); send() }}>
    {#if hits.length}
      <div class="mentions" id="mentions" role="listbox" aria-label={tr('Notes et dossiers')}>
        {#each hits as n, i (n.path)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            id="mention-{i}"
            role="option"
            tabindex="-1"
            aria-selected={i === hi}
            onmousedown={(e) => e.preventDefault()}
            onmouseenter={() => (hi = i)}
            onclick={() => attach(n)}
          >
            {#if n.dir}<Folder size={13} strokeWidth={1.75} />{:else}<FileText size={13} strokeWidth={1.75} />{/if}
            <span>{n.dir ? n.name : title(n.path)}</span>
            {#if n.path.includes('/')}<small>{n.path.split('/').slice(0, -1).join('/')}</small>{/if}
          </div>
        {/each}
      </div>
    {/if}
    {#if atts.length || app.current}
      <div class="chips">
        {#if app.current}
          <span class="chip ctx" title={tr('Note ouverte, jointe automatiquement')}><FileText size={12} strokeWidth={1.75} />{title(app.current)}</span>
        {/if}
        {#each atts as a, i}
          {@const Icon = a.kind === 'text' ? ClipboardPaste : a.kind === 'dir' ? Folder : FileText}
          <span class="chip" title={a.kind === 'text' ? a.text.slice(0, 300) : a.path}>
            <Icon size={12} strokeWidth={1.75} />{attName(a)}
            <button type="button" aria-label={tr('Retirer {name}', { name: attName(a) })} onclick={() => { atts.splice(i, 1); box.focus() }}><X size={11} strokeWidth={2} /></button>
          </span>
        {/each}
      </div>
    {/if}
    <textarea
      bind:this={box}
      bind:value={input}
      rows="1"
      placeholder={none ? tr('Installe Claude Code ou Codex pour discuter') : tr('Demander à {agent} — @ pour joindre une note', { agent: label })}
      disabled={none}
      role="combobox"
      aria-expanded={hits.length > 0}
      aria-controls="mentions"
      aria-autocomplete="list"
      aria-activedescendant={hits.length ? `mention-${hi}` : undefined}
      oninput={() => { grow(); seek() }}
      onclick={seek}
      onblur={() => (mention = null)}
      onpaste={paste}
      onkeydown={keys}
    ></textarea>
    <div class="bar">
      <div class="pickers">
        <DropdownMenu.Root onOpenChange={(o) => o && probe()}>
          <DropdownMenu.Trigger class="picker agent-pick" disabled={running || none} aria-label={tr('Agent : {name}', { name: label })}>
            {label}<ChevronDown size={12} strokeWidth={1.75} />
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content class="menu" side="top" align="start" sideOffset={6} collisionPadding={8} onCloseAutoFocus={(e) => { e.preventDefault(); box.focus() }}>
              {#each agents as a, i (a)}
                {#if i === 2}<DropdownMenu.Separator />{/if}
                {@const why = typeof models[a] === 'string' ? (models[a] as string) : undefined}
                <DropdownMenu.Item disabled={!usable(a)} onSelect={() => pick(a)} title={found[a] === false ? why : undefined}>
                  <Check size={13} strokeWidth={2} style="visibility: {agent === a ? 'visible' : 'hidden'}" />{nameOf(a)}
                  {#if found[a] === false}<small class="missing">{tr(a.startsWith('custom:') ? 'injoignable' : 'introuvable')}</small>{/if}
                </DropdownMenu.Item>
              {/each}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
        {#if !none}
          <DropdownMenu.Root onOpenChange={(o) => o && loadModels(agent)}>
            <DropdownMenu.Trigger class="picker" disabled={running} aria-label={tr('Modèle : {name}', { name: current(agent)?.name ?? tr('par défaut') })}>
              <span>{current(agent)?.name ?? tr(typeof models[agent] === 'string' ? 'Modèle par défaut' : 'Modèle…')}</span><ChevronDown size={12} strokeWidth={1.75} />
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content class="menu" side="top" align="start" sideOffset={6} collisionPadding={8} onCloseAutoFocus={(e) => { e.preventDefault(); box.focus() }}>
                {@const list = models[agent]}
                {#if Array.isArray(list)}
                  {#each list as m (m.id)}
                    <DropdownMenu.Item onSelect={() => choose(m)}>
                      <Check size={13} strokeWidth={2} style="visibility: {current(agent)?.id === m.id ? 'visible' : 'hidden'}" />{m.name}
                    </DropdownMenu.Item>
                  {/each}
                {:else}
                  <div class="note">{list ?? tr('Chargement des modèles…')}</div>
                {/if}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        {/if}
      </div>
      {#if running}
        <button type="button" class="send stop" title={tr('Arrêter')} onclick={() => call('agent_stop')}><Square size={11} fill="currentColor" strokeWidth={0} /></button>
      {:else}
        <button type="submit" class="send" title={tr('Envoyer')} disabled={none || (!input.trim() && !atts.length)}><ArrowUp size={15} strokeWidth={2} /></button>
      {/if}
    </div>
  </form>
</aside>

<style>
  .agent {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-deep);
    border-left: 1px solid var(--line-soft);
    min-width: 0;
  }
  header {
    display: flex;
    justify-content: flex-end;
    padding: 8px 8px 0;
  }

  .lines {
    flex: 1;
    overflow-y: auto;
    padding: 8px 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    user-select: text;
  }

  .intro { margin-top: 8px; display: grid; gap: 6px; justify-items: start; }
  .intro p { margin: 0 0 8px; color: var(--muted); font: var(--chat-size)/1.55 var(--text); }
  .intro button {
    text-align: left;
    padding: 6px 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r);
    color: var(--muted);
    transition: border-color var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .intro button:hover { border-color: var(--orange); color: var(--fg); }

  .user {
    align-self: flex-end;
    max-width: 88%;
    padding: 8px 12px;
    background: var(--surface);
    border: 1px solid var(--line-soft);
    border-radius: 8px;
    font: var(--chat-size)/1.5 var(--text);
    white-space: pre-wrap;
    animation: rise var(--t) var(--ease);
  }
  .ai { font: var(--chat-size)/1.62 var(--text); color: var(--fg); overflow-wrap: anywhere; }
  .ai :global(p) { margin: 0 0 0.6em; }
  .ai :global(p:last-child) { margin-bottom: 0; }
  .ai :global(ul), .ai :global(ol) { margin: 0.3em 0 0.6em; padding-left: 1.3em; }
  .ai :global(code) { font: 0.86em var(--mono); background: var(--surface); border-radius: 4px; padding: 1px 4px; }
  .ai :global(pre) { background: var(--surface); padding: 10px 12px; border-radius: var(--r); overflow-x: auto; }
  .ai :global(pre code) { background: none; padding: 0; }
  .ai :global(a) { color: var(--orange); }
  .ai :global(h1), .ai :global(h2), .ai :global(h3) { font: 650 15px/1.4 var(--display); margin: 0.8em 0 0.3em; }
  .ai.open > :global(:last-child::after) {
    content: '';
    display: inline-block;
    width: 2px;
    height: 1em;
    margin-left: 2px;
    vertical-align: -2px;
    background: var(--orange);
    animation: blink 1s steps(1) infinite;
  }
  @keyframes blink { 50% { opacity: 0; } }

  .action {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    color: var(--faint);
    font-size: 12.5px;
    animation: rise var(--t) var(--ease);
  }
  .action :global(svg) { flex: none; color: var(--orange); }
  .action strong { font-weight: 600; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .action:not(:disabled):hover strong { color: var(--fg); text-decoration: underline; text-underline-offset: 3px; }

  .error {
    padding: 8px 10px;
    border-left: 2px solid var(--orange);
    color: var(--muted);
    font: 12.5px/1.5 var(--text);
    white-space: pre-wrap;
  }

  .dots { display: flex; gap: 4px; padding: 6px 0; }
  .dots i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--orange);
    animation: dot 1.1s var(--ease) infinite;
  }
  .dots i:nth-child(2) { animation-delay: 0.15s; }
  .dots i:nth-child(3) { animation-delay: 0.3s; }
  @keyframes dot { 0%, 100% { opacity: 0.25; transform: none; } 40% { opacity: 1; transform: translateY(-2px); } }
  @keyframes rise { from { opacity: 0; transform: translateY(4px); } }

  .composer {
    position: relative;
    margin: 0 12px 12px;
    padding: 10px 10px 8px 12px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
    transition: border-color var(--t-fast) var(--ease);
  }
  .composer:focus-within { border-color: color-mix(in srgb, var(--orange) 70%, var(--line)); }
  textarea {
    display: block;
    width: 100%;
    resize: none;
    border: 0;
    background: none;
    font: var(--chat-size)/1.5 var(--text);
    max-height: 200px;
  }
  textarea::placeholder { color: var(--faint); }
  textarea:focus-visible { outline: none; }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-top: 6px; }
  .pickers { display: flex; align-items: center; gap: 2px; min-width: 0; margin-left: -5px; }
  .bar :global(.picker) {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    padding: 2px 5px;
    border-radius: 4px;
    font-size: 12px;
    color: var(--faint);
    white-space: nowrap;
    transition: color var(--t-fast) var(--ease), background var(--t-fast) var(--ease);
  }
  .bar :global(.agent-pick) { flex: none; font-weight: 600; }
  .bar :global(.picker span) { overflow: hidden; text-overflow: ellipsis; }
  .bar :global(.picker svg) { flex: none; }
  .bar :global(.picker:not(:disabled):hover), .bar :global(.picker[data-state='open']) { color: var(--fg); background: var(--surface); }

  .chips { display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 8px; }
  .user .chips { margin-bottom: 6px; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    padding: 2px 4px 2px 7px;
    border: 1px solid var(--line-soft);
    border-radius: 5px;
    background: var(--surface);
    font: 12px/1.5 var(--display);
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .user .chip { padding-right: 7px; background: var(--bg); }
  .chip :global(svg) { flex: none; color: var(--orange); }
  .chip.ctx { padding-right: 7px; border-style: dashed; background: none; color: var(--faint); }
  .chip button { display: grid; place-items: center; padding: 2px; border-radius: 3px; color: var(--faint); }
  .chip button:hover { color: var(--fg); background: var(--line-soft); }

  .mentions {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + 6px);
    padding: 4px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.12);
    animation: rise var(--t-fast) var(--ease);
  }
  .mentions [role='option'] {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 8px;
    border-radius: 5px;
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
  }
  .mentions [role='option'] :global(svg) { flex: none; color: var(--muted); }
  .mentions [role='option'] small { color: var(--faint); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; }
  .mentions [aria-selected='true'] { background: var(--orange-wash); }
  .send {
    display: grid;
    place-items: center;
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--orange);
    color: var(--bg);
    transition: opacity var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .send:disabled { opacity: 0.3; }
  .send:not(:disabled):active { transform: scale(.92); }
  .send.stop { background: var(--fg); color: var(--bg); }
</style>
