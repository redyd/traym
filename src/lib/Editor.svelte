<script lang="ts">
  import { onMount } from 'svelte'
  import { Editor, Extension } from '@tiptap/core'
  import { Markdown } from '@tiptap/markdown'
  import { Heading1, Heading2, Heading3, Bold, Italic, Strikethrough, Code, SquareCode, TextQuote, List, ListChecks } from '@lucide/svelte'
  import { extensions, split } from './schema'
  import { app, call, hooks, onDiskChange, title, mod, move, parent, join, flash, exists } from './store.svelte'

  let host: HTMLDivElement
  let scroller: HTMLDivElement
  let titleEl = $state<HTMLInputElement>()
  let editor: Editor | null = null
  let path: string | null = null
  let front = '' // frontmatter, kept verbatim outside the editor
  let saved = ''
  let timer = 0
  let words = $state(0)
  let status = $state<'saved' | 'saving'>('saved')
  let ember = $state(0)
  let section: HTMLElement

  const INLINE = ['bold', 'italic', 'strike', 'code']
  /** One inline style at a time: switching to another drops the current one. */
  function mark(name: string) {
    const e = editor!, sel = e.state.selection
    const c = e.chain().focus()
    // caret in the middle of styled text: act on that whole run; at its end (still typing): only on what comes next
    const inside = (m: string) => sel.empty && !!sel.$from.nodeAfter?.marks.some((x) => x.type.name === m)
    if (e.isActive(name))
      return (inside(name) ? c.extendMarkRange(name).unsetMark(name).setTextSelection(sel.from) : c.unsetMark(name)).run()
    for (const o of INLINE) {
      if (o === name || !e.isActive(o)) continue
      // swap: the run becomes this style instead
      if (inside(o)) return c.extendMarkRange(o).unsetMark(o).setMark(name).setTextSelection(sel.from).run()
      c.unsetMark(o)
    }
    return c.setMark(name).run()
  }

  /** Ctrl+Enter: back to plain text. Out of any block onto a new paragraph below it, else drop the inline style. */
  function exit() {
    const { state } = editor!
    const from = state.selection.$from
    if (from.depth > 1 || from.parent.type.name !== 'paragraph') {
      const end = from.after(1)
      return editor!.chain().insertContentAt(end, { type: 'paragraph' }).setTextSelection(end + 1).run()
    }
    if (!(state.storedMarks ?? from.marks()).length) return false
    editor!.view.dispatch(state.tr.setStoredMarks([]))
    return true
  }

  const keys = Extension.create({
    name: 'traymKeys',
    priority: 1000,
    addKeyboardShortcuts: () => ({
      'Mod-b': () => mark('bold'),
      'Mod-i': () => mark('italic'),
      'Mod-Shift-s': () => mark('strike'),
      'Mod-e': () => mark('code'),
      'Mod-Enter': exit,
    }),
  })

  type Tool = { key: string; label: string; icon: typeof Bold; run: () => unknown; active: () => boolean }
  const is = (name: string, attrs?: object) => () => !!editor?.isActive(name, attrs)
  const cmd = () => editor!.chain().focus()
  const tools: (Tool | null)[] = [
    { key: 'h1', label: 'Titre 1', icon: Heading1, run: () => cmd().toggleHeading({ level: 1 }).run(), active: is('heading', { level: 1 }) },
    { key: 'h2', label: 'Titre 2', icon: Heading2, run: () => cmd().toggleHeading({ level: 2 }).run(), active: is('heading', { level: 2 }) },
    { key: 'h3', label: 'Titre 3', icon: Heading3, run: () => cmd().toggleHeading({ level: 3 }).run(), active: is('heading', { level: 3 }) },
    null,
    { key: 'bold', label: `Gras (${mod}+B)`, icon: Bold, run: () => mark('bold'), active: is('bold') },
    { key: 'italic', label: `Italique (${mod}+I)`, icon: Italic, run: () => mark('italic'), active: is('italic') },
    { key: 'strike', label: `Barré (${mod}+Maj+S)`, icon: Strikethrough, run: () => mark('strike'), active: is('strike') },
    { key: 'code', label: `Code (${mod}+E)`, icon: Code, run: () => mark('code'), active: is('code') },
    null,
    { key: 'codeBlock', label: 'Bloc de code', icon: SquareCode, run: () => cmd().toggleCodeBlock().run(), active: is('codeBlock') },
    { key: 'quote', label: 'Citation', icon: TextQuote, run: () => cmd().toggleBlockquote().run(), active: is('blockquote') },
    { key: 'list', label: 'Liste', icon: List, run: () => cmd().toggleBulletList().run(), active: is('bulletList') },
    { key: 'task', label: 'Tâches', icon: ListChecks, run: () => cmd().toggleTaskList().run(), active: is('taskList') },
  ]
  let bar = $state<{ x: number; y: number } | null>(null)
  let on = $state<string[]>([])
  let dragging = false

  function place() {
    const sel = editor?.state.selection
    if (!editor || !sel || sel.empty || dragging || !editor.isFocused) return void (bar = null)
    const a = editor.view.coordsAtPos(sel.from), b = editor.view.coordsAtPos(sel.to)
    const box = section.getBoundingClientRect()
    const x = a.top === b.top ? (a.left + b.right) / 2 : a.left
    const above = a.top - box.top - 46
    bar = {
      x: Math.max(200, Math.min(box.width - 200, x - box.left)),
      y: above > 8 ? above : b.bottom - box.top + 10,
    }
  }

  // the editor always keeps an empty paragraph at the end: don't write it out as trailing blank lines
  const text = () => front + editor!.getMarkdown().replace(/\s+$/, '\n').replace(/^\n$/, '')
  const count = (t: string) => (t.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu) ?? []).length

  async function save() {
    clearTimeout(timer)
    if (!path || !editor) return
    const md = text()
    if (md === saved) return
    status = 'saving'
    await call('write_note', { path, content: md })
    saved = md
    status = 'saved'
  }
  hooks.flush = save

  // a fresh editor per note, so undo never reaches into the previous one
  function make(body: string) {
    editor?.destroy()
    editor = new Editor({
      element: host,
      extensions: [...extensions, Markdown, keys],
      content: body,
      contentType: 'markdown',
      onTransaction: () => {
        const a = tools.filter((t) => t?.active()).map((t) => t!.key)
        if (a.join() !== on.join()) on = a
        place()
      },
      onFocus: place,
      onBlur: place,
      onUpdate: ({ editor: e }) => {
        words = count(e.getText())
        status = 'saving'
        clearTimeout(timer)
        timer = window.setTimeout(save, 400)
      },
    })
    // the baseline is the editor's own rendering: opening a note never rewrites it
    saved = text()
    words = count(editor.getText())
  }

  async function load(next: string | null) {
    await save()
    path = next
    if (!next) return void (editor?.destroy(), editor = null)
    const disk = await call<string>('read_note', { path: next })
    if (path !== next) return
    const [f, body] = split(disk)
    front = f
    make(body)
    status = 'saved'
    scroller?.scrollTo({ top: 0 })
    host.animate([{ opacity: 0, transform: 'translateY(6px)' }, { opacity: 1, transform: 'none' }], {
      duration: 220,
      easing: 'cubic-bezier(.2,.8,.2,1)',
    })
    if (app.fresh === next) {
      app.fresh = null
      requestAnimationFrame(() => titleEl?.select())
    } else editor!.commands.focus('start')
  }

  async function rename(input: HTMLInputElement) {
    const name = input.value.trim().replace(/[\/]/g, '-')
    const from = app.current!
    const to = join(parent(from), name + '.md')
    if (!name || to === from) return void (input.value = title(from))
    if (exists(to)) {
      flash(`« ${name} » existe déjà ici`)
      return void (input.value = title(from))
    }
    await move(from, to)
  }

  $effect(() => {
    load(app.current)
  })

  onMount(() => {
    // no toolbar while the mouse is still dragging out a selection
    const down = () => { dragging = true; bar = null }
    const up = () => { if (dragging) { dragging = false; requestAnimationFrame(place) } }
    host.addEventListener('pointerdown', down)
    window.addEventListener('pointerup', up)
    const un = onDiskChange(async (paths) => {
      if (!path || !editor || !paths.includes(path)) return
      const disk = await call<string>('read_note', { path }).catch(() => null)
      // only take the disk version when the user has nothing unsaved
      if (disk === null || disk === saved || text() !== saved) return
      const [f, body] = split(disk)
      front = f
      const at = editor.state.selection.from
      editor.chain().setContent(body, { contentType: 'markdown', emitUpdate: false }).setTextSelection(Math.min(at, editor.state.doc.content.size)).run()
      saved = text()
      words = count(editor.getText())
      status = 'saved'
      ember++
    })
    return () => {
      window.removeEventListener('pointerup', up)
      save()
      un.then((f) => f())
      editor?.destroy()
    }
  })
</script>

{#snippet buttons()}
  {#each tools as t}
    {#if t}
      <button class="icon-btn" class:on={on.includes(t.key)} aria-pressed={on.includes(t.key)} title={t.label} aria-label={t.label} onclick={() => t.run()}><t.icon size={15} strokeWidth={1.6} /></button>
    {:else}
      <span class="sep"></span>
    {/if}
  {/each}
{/snippet}

<section class="editor" class:empty={!app.current} bind:this={section}>
  {#if app.current}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="toolbar" role="toolbar" tabindex="-1" onmousedown={(e) => e.preventDefault()}>
      {@render buttons()}
    </div>
  {/if}
  {#key ember}
    {#if ember}<div class="ember"></div>{/if}
  {/key}
  <div class="scroll" bind:this={scroller} onscroll={place}>
    <div class="page">
    {#if app.current}
      <input
        class="title"
        bind:this={titleEl}
        value={title(app.current)}
        spellcheck="false"
        aria-label="Titre de la note"
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === 'ArrowDown') { e.preventDefault(); editor?.commands.focus('start') }
          if (e.key === 'Escape') { e.currentTarget.value = title(app.current!); editor?.commands.focus() }
        }}
        onchange={(e) => rename(e.currentTarget)}
      />
    {/if}
    <div bind:this={host}></div>
    </div>
  </div>

  {#if bar}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="bar" role="toolbar" tabindex="-1" style:left="{bar.x}px" style:top="{bar.y}px" onmousedown={(e) => e.preventDefault()}>
      {@render buttons()}
    </div>
  {/if}

  {#if app.current}
    <footer>
      <span>{words} {words === 1 ? 'mot' : 'mots'}</span>
      <span class="state" class:busy={status === 'saving'}>{status === 'saved' ? 'Enregistré' : 'Enregistrement'}</span>
    </footer>
  {:else}
    <div class="blank">
      <p>Aucune note ouverte</p>
      <dl>
        <div><dt>Ouvrir une note</dt><dd><kbd>{mod}</kbd><kbd>P</kbd></dd></div>
        <div><dt>Nouvelle note</dt><dd><kbd>{mod}</kbd><kbd>N</kbd></dd></div>
        <div><dt>Commandes</dt><dd><kbd>{mod}</kbd><kbd>K</kbd></dd></div>
        <div><dt>Agent</dt><dd><kbd>{mod}</kbd><kbd>J</kbd></dd></div>
      </dl>
    </div>
  {/if}
</section>

<style>
  .editor {
    position: relative;
    height: 100%;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: 56px 32px 30vh;
  }
  .empty .scroll { display: none; }

  .title {
    display: block;
    width: 100%;
    margin: 0 0 22px;
    padding: 0 0 16px;
    border: 0;
    border-bottom: 1px solid var(--line-soft);
    background: none;
    font: 680 34px/1.15 var(--display);
    letter-spacing: -0.022em;
    color: var(--fg);
  }
  .title:focus-visible { outline: none; }

  .page { max-width: 700px; margin: 0 auto; }

  /* the one loud moment: a line of ember when an agent rewrites the open note */
  .ember {
    position: absolute;
    inset: 0 0 auto;
    height: 2px;
    background: var(--orange);
    transform-origin: left;
    animation: ember 1.1s var(--ease) forwards;
    z-index: 2;
    pointer-events: none;
  }
  @keyframes ember {
    0% { transform: scaleX(0); opacity: 1; }
    55% { transform: scaleX(1); opacity: 1; }
    100% { transform: scaleX(1); opacity: 0; }
  }

  .bar {
    position: absolute;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 1px;
    padding: 3px;
    transform: translateX(-50%);
    background: var(--surface-solid);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 8px 24px -10px #00000066;
    animation: bar 120ms var(--ease);
  }
  .toolbar {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 1px;
    height: 40px;
    border-bottom: 1px solid var(--line-soft);
  }
  .bar .icon-btn, .toolbar .icon-btn { width: 28px; height: 28px; color: var(--muted); }
  .bar .icon-btn.on, .toolbar .icon-btn.on { color: var(--orange); background: color-mix(in srgb, var(--orange) 12%, transparent); }
  .bar .sep, .toolbar .sep { width: 1px; height: 16px; margin: 0 3px; background: var(--line-soft); }
  @keyframes bar { from { opacity: 0; transform: translate(-50%, 3px); } }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 16px;
    padding: 6px 16px 8px;
    font: 500 11.5px/1 var(--display);
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }
  .state { transition: color var(--t) var(--ease); }
  .state.busy { color: var(--muted); }

  .blank {
    flex: 1;
    display: grid;
    place-content: center;
    gap: 20px;
    color: var(--faint);
  }
  .blank p {
    margin: 0;
    font: 600 15px/1 var(--display);
    color: var(--muted);
  }
  .blank dl { margin: 0; display: grid; gap: 10px; min-width: 240px; }
  .blank dl div { display: flex; justify-content: space-between; align-items: center; gap: 24px; }
  .blank dd { margin: 0; display: flex; gap: 4px; }


  /* ---------- document ---------- */
  :global(.tiptap) {
    outline: none;
    font: 400 16.5px/1.72 var(--text);
    color: var(--fg);
    caret-color: var(--orange);
  }
  :global(.tiptap ::selection) { background: var(--select); }
  :global(.tiptap > *) { margin: 0 0 0.55em; }
  :global(.tiptap p) { margin-block: 0 0.55em; }
  :global(.tiptap li p) { margin: 0; }

  :global(.tiptap :is(h1, h2, h3, h4, h5, h6)) { font-family: var(--display); color: var(--fg); letter-spacing: -0.015em; margin: 0.9em 0 0.3em; }
  :global(.tiptap > :is(h1, h2, h3):first-child) { margin-top: 0; }
  :global(.tiptap h1) { font-size: 1.62em; font-weight: 680; line-height: 1.3; }
  :global(.tiptap h2) { font-size: 1.32em; font-weight: 650; line-height: 1.35; }
  :global(.tiptap h3) { font-size: 1.12em; font-weight: 640; }
  :global(.tiptap :is(h4, h5, h6)) { font-size: 1em; font-weight: 640; color: var(--muted); }

  :global(.tiptap strong) { font-weight: 650; }
  :global(.tiptap s) { color: var(--faint); }
  :global(.tiptap code) {
    font: 0.86em var(--mono);
    background: var(--surface);
    border: 1px solid var(--line-soft);
    border-radius: 4px;
    padding: 1px 4px;
  }
  :global(.tiptap a) {
    color: var(--orange);
    text-decoration-color: color-mix(in srgb, var(--orange) 40%, transparent);
    text-underline-offset: 3px;
  }

  :global(.tiptap pre) {
    font: 400 13.5px/1.65 var(--mono);
    background: var(--surface);
    border: 1px solid var(--line-soft);
    border-radius: var(--r);
    padding: 8px 16px;
    white-space: pre-wrap;
  }
  :global(.tiptap pre code) { font: inherit; background: none; border: 0; padding: 0; }

  :global(.tiptap blockquote) {
    border-left: 2px solid var(--orange);
    padding-left: 16px;
    margin-left: 0;
    color: var(--muted);
  }
  :global(.tiptap blockquote > :last-child) { margin-bottom: 0; }

  :global(.tiptap :is(ul, ol)) { padding-left: 1.4em; }
  :global(.tiptap li::marker) { color: var(--faint); }
  :global(.tiptap ul[data-type='taskList']) { list-style: none; padding-left: 0.2em; }
  :global(.tiptap ul[data-type='taskList'] > li) { display: flex; gap: 8px; }
  :global(.tiptap ul[data-type='taskList'] > li > label) { flex: none; padding-top: 0.32em; }
  :global(.tiptap ul[data-type='taskList'] > li > div) { flex: 1; min-width: 0; }
  :global(.tiptap li[data-checked='true'] > div) { color: var(--faint); }
  :global(.tiptap input[type='checkbox']) {
    appearance: none;
    display: block;
    width: 15px;
    height: 15px;
    margin: 0;
    border: 1.5px solid var(--faint);
    border-radius: 4px;
    cursor: pointer;
    transition: background var(--t-fast) var(--ease), border-color var(--t-fast) var(--ease);
  }
  :global(.tiptap input[type='checkbox']:hover) { border-color: var(--orange); }
  :global(.tiptap input[type='checkbox']:checked) {
    background: var(--orange)
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Cpath d='M4 8.5l2.5 2.5L12 5.5' fill='none' stroke='white' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E")
      center / 12px no-repeat;
    border-color: var(--orange);
  }

  :global(.tiptap hr) { border: 0; border-top: 1px solid var(--line); margin: 1.2em 0; }
  :global(.tiptap hr.ProseMirror-selectednode) { border-top-color: var(--orange); }
  :global(.tiptap img) { max-width: 100%; border-radius: var(--r); }
  :global(.tiptap img.ProseMirror-selectednode) { outline: 2px solid var(--orange); }

  :global(.tiptap table) { border-collapse: collapse; width: 100%; table-layout: fixed; }
  :global(.tiptap :is(td, th)) { border: 1px solid var(--line-soft); padding: 4px 8px; vertical-align: top; }
  :global(.tiptap th) { font-weight: 650; text-align: left; background: var(--surface); }
  :global(.tiptap :is(td, th) p) { margin: 0; }
  :global(.tiptap .selectedCell) { background: var(--select); }
  :global(.tiptap .tableWrapper) { overflow-x: auto; }
</style>
