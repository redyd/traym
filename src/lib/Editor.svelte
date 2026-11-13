<script lang="ts">
  import { onMount } from 'svelte'
  import { Editor, Extension, type ChainedCommands } from '@tiptap/core'
  import { Markdown } from '@tiptap/markdown'
  import { BubbleMenu } from '@tiptap/extension-bubble-menu'
  import { FileHandler } from '@tiptap/extension-file-handler'
  import { DropdownMenu } from 'bits-ui'
  import { convertFileSrc } from '@tauri-apps/api/core'
  import { open } from '@tauri-apps/plugin-dialog'
  import {
    Heading1, Heading2, Heading3, Bold, Italic, Strikethrough, Code, SquareCode, TextQuote, List, ListChecks,
    Table, Image, BetweenHorizontalStart, BetweenHorizontalEnd, BetweenVerticalStart, BetweenVerticalEnd, Trash2, Grid2x2X, GripVertical, GripHorizontal, Plus,
  } from '@lucide/svelte'
  import { extensions, split, media } from './schema'
  import Tip from './Tip.svelte'
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
  let page: HTMLElement
  let caretEl = $state<HTMLElement>()
  let caret = $state<{ x: number; y: number; h: number } | null>(null)

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

  /** Enter does its usual job (split, continue a list…), then the new line starts as plain text:
   *  no carried-over bold/italic/strike/code, and a heading's next line is a paragraph. Code blocks keep Enter as is. */
  let entering = false
  function enter() {
    const e = editor!
    if (entering || e.isActive('codeBlock')) return false
    entering = true
    try {
      if (!e.commands.keyboardShortcut('Enter')) return false
    } finally {
      entering = false
    }
    const c = e.chain()
    if (e.isActive('heading')) c.setParagraph()
    return c.command(({ tr }) => (tr.setStoredMarks([]), true)).run()
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
      Enter: enter,
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
    null,
    { key: 'table', label: 'Tableau', icon: Table, run: () => cmd().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(), active: is('table') },
    { key: 'image', label: 'Image', icon: Image, run: () => image(), active: is('image') },
  ]
  // Notion-style table controls, shown on hover only: a grip beside the hovered row / above the hovered
  // column opens a small menu, and thin "+" strips under / right of the table add a row / column at the end
  type Hover = { cell: HTMLElement; table: HTMLTableElement; tx: number; ty: number; tw: number; th: number; ry: number; rh: number; cx: number; cw: number }
  let hov = $state<Hover | null>(null)
  let menu = $state<'row' | 'col' | null>(null)
  let hideT = 0

  function track(e: MouseEvent) {
    if (menu) return
    const cell = (e.target as HTMLElement).closest?.('td, th') as HTMLElement | null
    const table = cell?.closest('table')
    if (!cell || !table) return leave()
    clearTimeout(hideT)
    const p = page.getBoundingClientRect(), t = table.getBoundingClientRect(), c = cell.getBoundingClientRect()
    hov = { cell, table, tx: t.left - p.left, ty: t.top - p.top, tw: t.width, th: t.height, ry: c.top - p.top, rh: c.height, cx: c.left - p.left, cw: c.width }
  }
  // a short grace period lets the pointer travel from a cell to its grip outside the table
  const leave = () => { if (menu) return; clearTimeout(hideT); hideT = window.setTimeout(() => (hov = null), 300) }
  const stay = () => clearTimeout(hideT)

  /** Run a table command as if the caret were in `cell`. */
  function at(cell: HTMLElement, f: (c: ChainedCommands) => ChainedCommands) {
    f(editor!.chain().focus().setTextSelection(editor!.view.posAtDOM(cell, 0) + 1)).run()
    menu = null
    hov = null
  }
  const lastRow = (t: HTMLTableElement) => t.rows[t.rows.length - 1].cells[0] as HTMLElement
  const lastCol = (t: HTMLTableElement) => t.rows[0].cells[t.rows[0].cells.length - 1] as HTMLElement

  type Item = { label: string; icon: typeof Bold; run: (c: ChainedCommands) => ChainedCommands; danger?: boolean }
  const rowMenu: Item[] = [
    { label: 'Insérer au-dessus', icon: BetweenHorizontalStart, run: (c) => c.addRowBefore() },
    { label: 'Insérer en dessous', icon: BetweenHorizontalEnd, run: (c) => c.addRowAfter() },
    { label: 'Supprimer la ligne', icon: Trash2, run: (c) => c.deleteRow(), danger: true },
  ]
  const colMenu: Item[] = [
    { label: 'Insérer à gauche', icon: BetweenVerticalStart, run: (c) => c.addColumnBefore() },
    { label: 'Insérer à droite', icon: BetweenVerticalEnd, run: (c) => c.addColumnAfter() },
    { label: 'Supprimer la colonne', icon: Trash2, run: (c) => c.deleteColumn(), danger: true },
  ]
  // only while the caret is in a table: the same actions, reachable from the keyboard (the grips are mouse-only)
  const tableTools: Item[] = [...rowMenu, ...colMenu, { label: 'Supprimer le tableau', icon: Grid2x2X, run: (c) => c.deleteTable(), danger: true }]
  function openMenu(k: 'row' | 'col', o: boolean) {
    menu = o ? k : null
    if (!o) leave()
  }

  // vault-relative path of `to`, written relative to the note's folder `dir`
  function relTo(dir: string, to: string) {
    const a = dir ? dir.split('/') : [], b = to.split('/')
    let i = 0
    while (i < a.length && i < b.length - 1 && a[i] === b[i]) i++
    return [...a.slice(i).map(() => '..'), ...b.slice(i)].join('/')
  }

  const IMAGES = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'avif']
  /** Embed vault files `rels` in `note` (if still the open one), at `pos` or the caret. */
  function embed(note: string, rels: string[], pos?: number) {
    if (note !== path || !rels.length) return // the user moved to another note meanwhile
    const dir = parent(note)
    const nodes = rels.map((rel) => ({ type: 'image', attrs: { src: encodeURI(relTo(dir, rel)), alt: rel.split('/').pop()!.replace(/\.\w+$/, '') } }))
    ;(pos == null ? cmd().insertContent(nodes) : cmd().insertContentAt(pos, nodes)).run()
  }

  /** Embed image files from disk: one already in the vault is linked, one from outside is copied next to the note first. */
  async function embedPaths(files: string[], pos?: number) {
    const note = path
    if (!note || !app.vault) return
    const rels: string[] = []
    for (const f of files)
      rels.push(f.startsWith(app.vault + '/') ? f.slice(app.vault.length + 1) : await call<string>('import_file', { src: f, dir: parent(note) }))
    embed(note, rels, pos)
  }

  async function image() {
    const file = await open({ filters: [{ name: 'Images', extensions: IMAGES }] })
    if (typeof file === 'string') embedPaths([file])
  }

  /** WebKitGTK hands over files copied or dragged from the file manager as file:// URIs, not as files. */
  function imagePaths(dt: DataTransfer | null) {
    const uris = (dt?.getData('text/uri-list') || dt?.getData('text/plain') || '').split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith('#'))
    if (!uris.length || !uris.every((u) => u.startsWith('file://'))) return []
    return uris.map((u) => decodeURIComponent(new URL(u).pathname)).filter((p) => IMAGES.includes(p.split('.').pop()!.toLowerCase()))
  }

  /** Pasted or dropped images: written next to the note, then embedded where they landed. */
  async function drop(files: File[], pos?: number) {
    const note = path
    if (!note) return
    const dir = parent(note)
    const rels: string[] = []
    for (const f of files) {
      // a pasted screenshot has no real name
      const name = f.name && f.name !== 'image.png' ? f.name : `image-${new Date().toISOString().slice(0, 19).replace(/\D/g, '')}.${f.type.split('/')[1]?.replace('svg+xml', 'svg') ?? 'png'}`
      rels.push(await call<string>('save_file', new Uint8Array(await f.arrayBuffer()), {
        headers: { 'x-dir': encodeURIComponent(dir), 'x-name': encodeURIComponent(name) },
      }))
    }
    embed(note, rels, pos)
  }
  const files = FileHandler.configure({
    allowedMimeTypes: IMAGES.map((e) => `image/${{ jpg: 'jpeg', svg: 'svg+xml' }[e] ?? e}`),
    onPaste: (_e, f) => void drop(f),
    onDrop: (_e, f, pos) => void drop(f, pos),
    consumePasteEvent: true,
  })

  // Word-style caret: our own bar that glides to its new spot (the native one jumps), blinking softly
  function moveCaret() {
    const sel = editor?.state.selection
    if (!editor || !sel?.empty || !editor.isFocused) return void (caret = null)
    const c = editor.view.coordsAtPos(sel.head), box = page.getBoundingClientRect()
    caret = { x: c.left - box.left, y: c.top - box.top, h: c.bottom - c.top }
    // restart only the blink so the caret stays solid while typing. getAnimations() also lists the glide
    // transition, and rewinding that one snapped the caret back to where it came from
    caretEl?.getAnimations().forEach((a) => { if (!(a instanceof CSSTransition)) a.currentTime = 0 })
  }
  let barEl: HTMLDivElement
  let on = $state<string[]>([])
  let dragging = false
  // selection toolbar: Tiptap's bubble menu, placed by floating-ui (flips below / shifts sideways to stay in view)
  const bubble = () =>
    BubbleMenu.configure({
      element: barEl,
      // inside the scrolled page, so it scrolls with the text natively instead of chasing it from scroll events
      appendTo: () => page,
      shouldShow: ({ editor: e, state }) =>
        !dragging && e.isEditable && !state.selection.empty && (e.view.hasFocus() || barEl.contains(document.activeElement)),
      options: { strategy: 'absolute', placement: 'top', offset: 8, flip: { padding: 8 }, shift: { padding: 8 }, hide: true, scrollTarget: scroller },
    })

  // the editor always keeps an empty paragraph at the end: don't write it out as trailing blank lines
  const text = () => front + editor!.getMarkdown().replace(/\s*$/, '\n').replace(/^\n$/, '')
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
      extensions: [...extensions, Markdown, keys, bubble(), files],
      content: body,
      contentType: 'markdown',
      // these run before plugins, so file:// images are taken here and real files fall through to FileHandler
      editorProps: {
        handlePaste: (_v, e) => {
          const p = imagePaths(e.clipboardData)
          if (p.length) embedPaths(p)
          return p.length > 0
        },
        handleDrop: (v, e) => {
          const p = imagePaths(e.dataTransfer)
          if (!p.length) return false
          e.preventDefault()
          embedPaths(p, v.posAtCoords({ left: e.clientX, top: e.clientY })?.pos)
          return true
        },
      },
      onTransaction: () => {
        const a = tools.filter((t) => t?.active()).map((t) => t!.key)
        if (a.join() !== on.join()) on = a
        moveCaret()
      },
      onFocus: moveCaret,
      onBlur: moveCaret,
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
    // images are written relative to the note; the webview reads them through Tauri's asset protocol
    const base = `${app.vault}/${parent(next)}`.replace(/\/$/, '')
    media.resolve = (src) => {
      if (/^[a-z][\w+.-]*:/i.test(src)) return src
      let p = src
      try { p = decodeURI(src) } catch {}
      return convertFileSrc(p.startsWith('/') ? p : `${base}/${p}`)
    }
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
    const down = () => { dragging = true }
    const up = () => {
      if (!dragging) return
      dragging = false
      // next frame: ProseMirror settles a click on mouseup, after pointerup; dispatching before that cancels the click
      requestAnimationFrame(() => {
        if (editor && !editor.state.selection.empty) editor.view.dispatch(editor.state.tr.setMeta('bubbleMenu', 'show'))
      })
    }
    host.addEventListener('pointerdown', down)
    window.addEventListener('pointerup', up)
    host.addEventListener('mousemove', track)
    host.addEventListener('mouseleave', leave)
    // reflow (window resize, images loading) moves text under a still caret
    const ro = new ResizeObserver(() => moveCaret())
    ro.observe(host)
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
      ro.disconnect()
      save()
      un.then((f) => f())
      editor?.destroy()
    }
  })
</script>

{#snippet buttons(table: boolean)}
  {#each tools as t}
    {#if t}
      <Tip class={on.includes(t.key) ? 'icon-btn on' : 'icon-btn'} aria-pressed={on.includes(t.key)} label={t.label} onclick={() => t.run()}><t.icon size={15} strokeWidth={1.6} /></Tip>
    {:else}
      <span class="sep"></span>
    {/if}
  {/each}
  {#if table && on.includes('table')}
    <span class="sep"></span>
    {#each tableTools as t}
      <Tip class="icon-btn" label={t.label} onclick={() => t.run(cmd()).run()}><t.icon size={15} strokeWidth={1.6} /></Tip>
    {/each}
  {/if}
{/snippet}

{#snippet tableMenu(k: 'row' | 'col', h: Hover, style: string)}
  <DropdownMenu.Root bind:open={() => menu === k, (o) => openMenu(k, o)}>
    <DropdownMenu.Trigger class={menu === k ? `grip ${k} open` : `grip ${k}`} aria-label={k === 'row' ? 'Options de la ligne' : 'Options de la colonne'} {style}>
      {#if k === 'row'}<GripVertical size={12} />{:else}<GripHorizontal size={12} />{/if}
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content class="menu" side={k === 'row' ? 'left' : 'bottom'} align="start" sideOffset={4} collisionPadding={8} onCloseAutoFocus={(e) => e.preventDefault()}>
        {#each k === 'row' ? rowMenu : colMenu as it}
          <DropdownMenu.Item class={it.danger ? 'danger' : ''} onSelect={() => at(h.cell, it.run)}><it.icon size={14} strokeWidth={1.6} />{it.label}</DropdownMenu.Item>
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
{/snippet}


<section class="editor" class:empty={!app.current}>
  {#if app.current}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="toolbar" role="toolbar" aria-label="Mise en forme" tabindex="-1" onmousedown={(e) => e.preventDefault()}>
      {@render buttons(true)}
    </div>
  {/if}
  {#key ember}
    {#if ember}<div class="ember"></div>{/if}
  {/key}
  <div class="scroll" bind:this={scroller}>
    <div class="page" bind:this={page}>
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
    {#if caret}
      <div class="caret" bind:this={caretEl} style:transform="translate({caret.x}px, {caret.y}px)" style:height="{caret.h}px"></div>
    {/if}
    {#if hov}
      {@const h = hov}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="tbl" onmouseenter={stay} onmouseleave={leave} onmousedown={(e) => e.preventDefault()}>
        {#if menu === 'row'}<div class="tsel" style:left="{h.tx}px" style:top="{h.ry}px" style:width="{h.tw}px" style:height="{h.rh}px"></div>{/if}
        {#if menu === 'col'}<div class="tsel" style:left="{h.cx}px" style:top="{h.ty}px" style:width="{h.cw}px" style:height="{h.th}px"></div>{/if}
        {@render tableMenu('row', h, `left:${h.tx - 22}px;top:${h.ry + h.rh / 2 - 10}px`)}
        {@render tableMenu('col', h, `left:${h.cx + h.cw / 2 - 10}px;top:${h.ty - 20}px`)}
        <Tip class="add" label="Ajouter une ligne" style="left:{h.tx}px;top:{h.ty + h.th + 4}px;width:{h.tw}px;height:14px" onclick={() => at(lastRow(h.table), (c) => c.addRowAfter())}><Plus size={11} /></Tip>
        <Tip class="add" label="Ajouter une colonne" style="left:{h.tx + h.tw + 4}px;top:{h.ty}px;width:14px;height:{h.th}px" onclick={() => at(lastCol(h.table), (c) => c.addColumnAfter())}><Plus size={11} /></Tip>
      </div>
    {/if}
    </div>
  </div>

  <!-- shown, hidden and placed by the bubble menu extension -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="bar" role="toolbar" aria-label="Mise en forme de la sélection" tabindex="-1" bind:this={barEl} style="visibility: hidden" onmousedown={(e) => e.preventDefault()}>
    {@render buttons(false)}
  </div>

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

  .page { position: relative; max-width: 700px; margin: 0 auto; }

  /* table controls: faint until pointed at */
  .tbl { display: contents; }
  .tbl :global(:is(.grip, .add)) {
    position: absolute;
    z-index: 3;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--faint);
    opacity: 0.7;
    cursor: pointer;
    animation: tfade 140ms var(--ease);
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease), opacity var(--t-fast) var(--ease);
  }
  .tbl :global(.grip) { width: 20px; height: 20px; }
  .tbl :global(:is(.grip:hover, .grip.open, .add:hover, .grip:focus-visible, .add:focus-visible)) { opacity: 1; color: var(--fg); background: var(--hover); }
  .tbl :global(.grip.open) { color: var(--orange); }
  .tsel {
    position: absolute;
    z-index: 2;
    pointer-events: none;
    border: 1.5px solid var(--orange);
    border-radius: 3px;
    background: color-mix(in srgb, var(--orange) 6%, transparent);
  }
  @keyframes tfade { from { opacity: 0; } }

  .caret {
    position: absolute;
    left: -1px;
    top: 0;
    width: 2px;
    border-radius: 1px;
    background: var(--orange);
    pointer-events: none;
    transition: transform 90ms var(--ease), height 90ms var(--ease);
    animation: blink 1.1s ease-in-out 0.5s infinite;
  }
  @keyframes blink { 0%, 40% { opacity: 1; } 55%, 85% { opacity: 0; } 100% { opacity: 1; } }

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
    top: 0;
    left: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 1px;
    padding: 3px;
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
  .bar :global(.icon-btn), .toolbar :global(.icon-btn) { width: 28px; height: 28px; color: var(--muted); }
  .bar :global(.icon-btn.on), .toolbar :global(.icon-btn.on) { color: var(--orange); background: color-mix(in srgb, var(--orange) 12%, transparent); }
  .bar .sep, .toolbar .sep { width: 1px; height: 16px; margin: 0 3px; background: var(--line-soft); }
  @keyframes bar { from { opacity: 0; } }

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
    caret-color: transparent; /* drawn by .caret */
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
  :global(.tiptap .tableWrapper) { overflow-x: auto; margin: 18px 0 22px; }
</style>
