<script lang="ts">
  import Tip from './Tip.svelte'
  import { onMount, tick } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'
  import { ArrowUp, Square, SquarePen, FilePen, FileText, FolderPlus, Search, Trash2, MoveRight, List } from '@lucide/svelte'
  import { app, call, pulse, reveal, title, flat } from './store.svelte'

  type Line = { role: 'user' | 'ai' | 'action' | 'error'; text: string; open?: boolean; path?: string | null; tool?: string }

  const ls = (k: string) => { try { return localStorage.getItem(k) } catch { return null } }
  let agent = $state<'claude' | 'codex'>(ls('agent') === 'codex' ? 'codex' : 'claude')
  let lines = $state<Line[]>([])
  let session: string | null = null
  let running = $state(false)
  let input = $state('')
  let list: HTMLDivElement
  let box: HTMLTextAreaElement

  const label = $derived(agent === 'codex' ? 'Codex' : 'Claude')
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
    if (!text || running) return
    input = ''
    lines.push({ role: 'user', text })
    running = true
    scroll()
    const ctx = app.current ? `\n\n[Note ouverte dans traym : ${app.current}]` : ''
    try {
      await call('agent_send', { agent, prompt: text + ctx, session })
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

  function pick(a: 'claude' | 'codex') {
    if (running || a === agent) return
    agent = a
    try { localStorage.setItem('agent', a) } catch {}
    reset() // sessions don't carry across CLIs
  }

  onMount(() => {
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
      lines.push({ role: 'action', text: verbs[payload.tool] ?? payload.tool, path: payload.path, tool: payload.tool })
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

  function grow() {
    box.style.height = 'auto'
    box.style.height = Math.min(box.scrollHeight, 200) + 'px'
  }

  const examples = ['Résume la note ouverte', 'Range mes notes par thème dans des dossiers', 'Crée une liste de courses dans Perso']
</script>

<aside class="agent">
  <header>
    <div class="seg" role="tablist" aria-label="Agent">
      <button role="tab" aria-selected={agent === 'claude'} onclick={() => pick('claude')}>Claude</button>
      <button role="tab" aria-selected={agent === 'codex'} onclick={() => pick('codex')}>Codex</button>
    </div>
    <Tip class="icon-btn" label="Nouvelle conversation" disabled={running} onclick={reset}><SquarePen size={15} strokeWidth={1.5} /></Tip>
  </header>

  <div class="lines" bind:this={list}>
    {#if !lines.length}
      <div class="intro">
        <p>{label} lit, écrit et range vos notes directement dans ce vault.</p>
        {#each examples as ex}
          <button onclick={() => send(ex)}>{ex}</button>
        {/each}
      </div>
    {/if}

    {#each lines as l, i (i)}
      {#if l.role === 'user'}
        <div class="user">{l.text}</div>
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
      <div class="dots" aria-label="{label} réfléchit"><i></i><i></i><i></i></div>
    {/if}
  </div>

  <form class="composer" onsubmit={(e) => { e.preventDefault(); send() }}>
    <textarea
      bind:this={box}
      bind:value={input}
      rows="1"
      placeholder="Demander à {label}"
      oninput={grow}
      onkeydown={(e) => {
        if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); requestAnimationFrame(grow) }
      }}
    ></textarea>
    <div class="bar">
      <span class="ctx">{app.current ? title(app.current) : 'Aucune note ouverte'}</span>
      {#if running}
        <button type="button" class="send stop" title="Arrêter" onclick={() => call('agent_stop')}><Square size={11} fill="currentColor" strokeWidth={0} /></button>
      {:else}
        <button type="submit" class="send" title="Envoyer" disabled={!input.trim()}><ArrowUp size={15} strokeWidth={2} /></button>
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
    align-items: center;
    justify-content: space-between;
    padding: 8px 8px 8px 12px;
  }
  .seg {
    display: flex;
    padding: 2px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r);
  }
  .seg button {
    padding: 4px 10px;
    border-radius: 4px;
    font-weight: 600;
    color: var(--faint);
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .seg button:hover { color: var(--fg); }
  .seg button[aria-selected='true'] { background: var(--surface); color: var(--fg); }

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
  .intro p { margin: 0 0 8px; color: var(--muted); font: 14px/1.55 var(--text); }
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
    font: 14px/1.5 var(--text);
    white-space: pre-wrap;
    animation: rise var(--t) var(--ease);
  }
  .ai { font: 14px/1.62 var(--text); color: var(--fg); overflow-wrap: anywhere; }
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
    font: 14px/1.5 var(--text);
    max-height: 200px;
  }
  textarea::placeholder { color: var(--faint); }
  textarea:focus-visible { outline: none; }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-top: 6px; }
  .ctx { font-size: 12px; color: var(--faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .send {
    display: grid;
    place-items: center;
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--orange);
    color: #fff;
    transition: opacity var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .send:disabled { opacity: 0.3; }
  .send:not(:disabled):active { transform: scale(.92); }
  .send.stop { background: var(--fg); color: var(--bg); }
</style>
