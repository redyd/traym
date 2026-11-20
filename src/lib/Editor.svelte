<script lang="ts">
  import { onMount } from 'svelte'
  import { Editor, Extension, type ChainedCommands } from '@tiptap/core'
  import { Markdown } from '@tiptap/markdown'
  import { BubbleMenu } from '@tiptap/extension-bubble-menu'
  import { FileHandler } from '@tiptap/extension-file-handler'
  import { DropdownMenu, ContextMenu } from 'bits-ui'
  import { convertFileSrc, invoke } from '@tauri-apps/api/core'
  import { open } from '@tauri-apps/plugin-dialog'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import {
    Heading1, Heading2, Heading3, Bold, Italic, Strikethrough, Code, SquareCode, TextQuote, List, ListChecks,
    Link, ExternalLink, Unlink, ListOrdered, Minus, Highlighter, Sigma, SquareSigma, Superscript, FileText, Table, Image, Grid2x2X, GripVertical, GripHorizontal, Plus, Scissors, Copy, ClipboardPaste, TextSelect, Pilcrow, ChevronRight, Eye, FileCode, Columns2, Undo2, Redo2,
  } from '@lucide/svelte'
  import TableIcon, { type TableOp } from './TableIcon.svelte'
  import { extensions, split, media, ui } from './schema'
  import Suggestion from '@tiptap/suggestion'
  import 'katex/dist/katex.min.css'
  import Tip from './Tip.svelte'
  import { t as tr } from './i18n.svelte'
  import { app, call, hooks, onDiskChange, title, mod, move, parent, join, flash, exists, persist, flat, refresh, setView, isNote, isImage, diskPaths, IMAGES, type View } from './store.svelte'

  let host: HTMLDivElement
  let scroller: HTMLDivElement
  let titleEl = $state<HTMLInputElement>()
  let editor: Editor | null = null
  let path: string | null = null
  let front = '' // frontmatter, kept verbatim outside the editor
  let saved = ''
  let src = $state('') // the file as text, for the source view
  let raw: string | null = null // set while the last edit came from the source view: saved verbatim, not re-serialized
  let srcEl = $state<HTMLTextAreaElement>()
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
      'Mod-l': () => (editLink(), true), // Mod-K is the command palette
      'Mod-Shift-h': () => editor!.chain().focus().toggleHighlight().run(),
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
    { key: 'bold', label: 'Gras ({mod}+B)', icon: Bold, run: () => mark('bold'), active: is('bold') },
    { key: 'italic', label: 'Italique ({mod}+I)', icon: Italic, run: () => mark('italic'), active: is('italic') },
    { key: 'strike', label: 'Barré ({mod}+Maj+S)', icon: Strikethrough, run: () => mark('strike'), active: is('strike') },
    { key: 'code', label: 'Code ({mod}+E)', icon: Code, run: () => mark('code'), active: is('code') },
    { key: 'highlight', label: 'Surligner ({mod}+Maj+H)', icon: Highlighter, run: () => cmd().toggleHighlight().run(), active: is('highlight') },
    { key: 'link', label: 'Lien ({mod}+L)', icon: Link, run: () => editLink(), active: is('link') },
    null,
    { key: 'codeBlock', label: 'Bloc de code', icon: SquareCode, run: () => cmd().toggleCodeBlock().run(), active: is('codeBlock') },
    { key: 'quote', label: 'Citation', icon: TextQuote, run: () => cmd().toggleBlockquote().run(), active: is('blockquote') },
    { key: 'list', label: 'Liste', icon: List, run: () => cmd().toggleBulletList().run(), active: is('bulletList') },
    { key: 'ordered', label: 'Liste numérotée', icon: ListOrdered, run: () => cmd().toggleOrderedList().run(), active: is('orderedList') },
    { key: 'task', label: 'Tâches', icon: ListChecks, run: () => cmd().toggleTaskList().run(), active: is('taskList') },
    null,
    { key: 'table', label: 'Tableau', icon: Table, run: () => cmd().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(), active: is('table') },
    { key: 'image', label: 'Image', icon: Image, run: () => image(), active: is('image') },
    { key: 'hr', label: 'Séparateur', icon: Minus, run: () => cmd().setHorizontalRule().run(), active: () => false },
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

  type Item = { label: string; icon: TableOp | typeof Bold; run: (c: ChainedCommands) => ChainedCommands; danger?: boolean }
  const rowMenu: Item[] = [
    { label: 'Insérer une ligne au-dessus', icon: 'row-above', run: (c) => c.addRowBefore() },
    { label: 'Insérer une ligne en dessous', icon: 'row-below', run: (c) => c.addRowAfter() },
    { label: 'Supprimer la ligne', icon: 'row-del', run: (c) => c.deleteRow(), danger: true },
  ]
  const colMenu: Item[] = [
    { label: 'Insérer une colonne à gauche', icon: 'col-left', run: (c) => c.addColumnBefore() },
    { label: 'Insérer une colonne à droite', icon: 'col-right', run: (c) => c.addColumnAfter() },
    { label: 'Supprimer la colonne', icon: 'col-del', run: (c) => c.deleteColumn(), danger: true },
  ]
  // only while the caret is in a table: the same actions, reachable from the keyboard (the grips are mouse-only)
  const tableTools: Item[] = [...rowMenu, ...colMenu, { label: 'Supprimer le tableau', icon: Grid2x2X, run: (c) => c.deleteTable(), danger: true }]
  // right-click menu: what the toolbar offers, plus the clipboard, scoped to where the click landed
  let picked = $state(false) // a non-empty selection
  const tool = (k: string) => tools.find((t) => t?.key === k)!
  const blocks = ['h1', 'h2', 'h3', 'list', 'ordered', 'task', 'quote', 'codeBlock'].map(tool)

  function aim(e: MouseEvent) {
    const v = editor?.view
    if (!v) return
    // right-clicking outside the selection moves the caret there, so "the row" means the row under the mouse
    const at = v.posAtCoords({ left: e.clientX, top: e.clientY })?.pos
    const { from, to, empty } = v.state.selection
    if (at != null && (empty || at < from || at > to)) editor!.chain().focus().setTextSelection(at).run()
    else editor!.commands.focus()
    picked = !editor!.state.selection.empty
  }

  function clip(action: 'cut' | 'copy') {
    editor!.view.focus()
    document.execCommand(action) // goes through ProseMirror's own copy/cut handlers (markdown-safe)
  }
  async function pasteClip() {
    editor!.view.focus()
    try {
      editor!.view.pasteText(await navigator.clipboard.readText())
    } catch {
      flash(tr('Collage impossible depuis le menu ici : utilise {mod}+V', { mod }))
    }
  }

  // links: a small field under the link (or the selection) to set, change or remove its target
  type Field = { x: number; y: number; value: string; hint: string; label: string; apply: (v: string) => unknown }
  let field = $state<Field | null>(null)
  function ask(pos: number, f: Omit<Field, 'x' | 'y'>) {
    const c = editor!.view.coordsAtPos(pos), p = page.getBoundingClientRect()
    field = { ...f, x: Math.max(0, Math.min(c.left - p.left, p.width - 340)), y: c.bottom - p.top + 6 }
  }
  function editLink() {
    const e = editor!
    e.chain().focus().extendMarkRange('link').run() // caret inside a link: take the whole link
    ask(e.state.selection.from, { value: e.getAttributes('link').href ?? '', hint: tr('https://… ou chemin/vers/note.md'), label: tr('Adresse du lien'), apply: applyLink })
  }
  function applyLink(href: string) {
    href = href.trim()
    const c = editor!.chain().focus().extendMarkRange('link')
    if (!href) return c.unsetLink().run()
    // nothing selected: the address becomes the link's text
    if (editor!.state.selection.empty) return c.insertContent({ type: 'text', text: href, marks: [{ type: 'link', attrs: { href } }] }).run()
    c.setLink({ href }).run()
  }

  // formulas: KaTeX, edited as LaTeX in the same field; clicking one opens it
  ui.math = (latex, pos, block) =>
    ask(pos, {
      value: latex, hint: 'e^{i\\pi} + 1 = 0', label: tr('Formule (LaTeX)'),
      apply: (v) => {
        const c = editor!.chain().focus()
        if (!v.trim()) return (block ? c.deleteBlockMath({ pos }) : c.deleteInlineMath({ pos })).run()
        ;(block ? c.updateBlockMath({ latex: v, pos }) : c.updateInlineMath({ latex: v, pos })).run()
      },
    })
  function newMath(block: boolean) {
    const e = editor!, { from, to } = e.state.selection
    const picked = e.state.doc.textBetween(from, to)
    ask(from, {
      value: picked, hint: 'e^{i\\pi} + 1 = 0', label: tr('Formule (LaTeX)'),
      apply: (v) => v.trim() && e.chain().focus().deleteRange({ from, to }).command(({ commands }) => (block ? commands.insertBlockMath({ latex: v, pos: from }) : commands.insertInlineMath({ latex: v, pos: from }))).run(),
    })
  }

  // footnotes: a [^n] here, its [^n]: definition at the end of the note, caret there to write it
  function footnote() {
    const e = editor!
    let n = 0
    e.state.doc.descendants((node) => { if (node.type.name === 'footnote') n = Math.max(n, Number(node.attrs.id) || 0) })
    const id = String(n + 1)
    e.chain().focus().insertContent({ type: 'footnoteRef', attrs: { id } }).run()
    // before the empty paragraph the editor keeps at the end (StarterKit's trailing node)
    const doc = e.state.doc, last = doc.lastChild!
    const end = last.type.name === 'paragraph' && !last.content.size ? doc.content.size - last.nodeSize : doc.content.size
    e.commands.insertContentAt(end, { type: 'footnote', attrs: { id } })
    toFootnote(id)
  }
  function toFootnote(id: string) {
    let at = -1
    editor!.state.doc.descendants((node, pos) => { if (at < 0 && node.type.name === 'footnote' && node.attrs.id === id) at = pos })
    if (at < 0) return flash(tr('Note de bas de page [^{id}] sans définition', { id }))
    editor!.chain().focus().setTextSelection(at + 1).scrollIntoView().run()
  }

  // [[wiki links]]: a note by name (anywhere in the vault, the nearest first) or by path
  const noExt = (p: string) => p.replace(/\.md$/, '')
  function wikiTarget(p: string) {
    const base = noExt(p.split('/').pop()!)
    return flat().filter(isNote).filter((q) => noExt(q.split('/').pop()!) === base).length > 1 ? noExt(p) : base
  }
  function resolveWiki(target: string) {
    const t = target.split('#')[0].trim().replace(/\.md$/, '')
    const all = flat().filter(isNote), here = parent(path ?? '')
    return all.find((p) => noExt(p) === t) ?? all.filter((p) => noExt(p).endsWith('/' + t) || noExt(p) === t).sort((a, b) => Number(parent(b) === here) - Number(parent(a) === here))[0]
  }
  async function openWiki(target: string) {
    let to = resolveWiki(target)
    if (!to) {
      // like Obsidian: following a link to a note that doesn't exist yet creates it, next to this one
      const name = target.split('#')[0].trim().replace(/\.md$/, '')
      if (!name) return
      to = name.includes('/') ? name + '.md' : join(parent(path ?? ''), name + '.md')
      try {
        await call('create_note', { path: to })
        await refresh()
      } catch {
        return
      }
    }
    app.current = to
    persist()
  }
  type Sug = { items: string[]; index: number; x: number; y: number; pick: (p: string) => void }
  let sug = $state<Sug | null>(null)
  const wikiSuggest = Extension.create({
    name: 'wikiSuggest',
    addProseMirrorPlugins() {
      return [Suggestion<string>({
        editor: this.editor,
        char: '[[',
        allowSpaces: true,
        allowedPrefixes: null,
        items: ({ query }) => {
          const q = query.toLowerCase()
          return flat().filter((p) => isNote(p) && p !== path && p.toLowerCase().includes(q)).slice(0, 8)
        },
        command: ({ editor: e, range, props }) => {
          e.chain().focus().insertContentAt(range, [{ type: 'wikiLink', attrs: { target: wikiTarget(props) } }, { type: 'text', text: ' ' }]).run()
        },
        render: () => {
          const show = (p: any) => {
            const r = p.clientRect?.(), pg = page.getBoundingClientRect()
            if (!r || !p.items.length) return void (sug = null)
            sug = { items: p.items, index: Math.min(sug?.index ?? 0, p.items.length - 1), x: r.left - pg.left, y: r.bottom - pg.top + 4, pick: (it) => p.command(it) }
          }
          return {
            onStart: show,
            onUpdate: show,
            onExit: () => (sug = null),
            onKeyDown: ({ event }) => {
              if (!sug) return false
              const n = sug.items.length
              if (event.key === 'ArrowDown') return (sug.index = (sug.index + 1) % n), true
              if (event.key === 'ArrowUp') return (sug.index = (sug.index + n - 1) % n), true
              if (event.key === 'Enter' || event.key === 'Tab') return sug.pick(sug.items[sug.index]), true
              if (event.key === 'Escape') return (sug = null), true
              return false
            },
          }
        },
      })]
    },
  })

  /** Ctrl/Cmd+click: web and mail links open in the system's apps, a relative link to a note opens it here. */
  function follow(href: string) {
    if (/^(https?|mailto):/i.test(href)) return void openUrl(href).catch((e) => flash(String(e)))
    if (/^[a-z][\w+.-]*:/i.test(href) || href.startsWith('#')) return // other schemes (file:…) and anchors: not followed
    let rel = href.split('#')[0]
    try { rel = decodeURI(rel) } catch {}
    const parts: string[] = rel.startsWith('/') ? [] : parent(path ?? '').split('/').filter(Boolean)
    for (const s of rel.split('/')) s === '..' ? parts.pop() : s && s !== '.' && parts.push(s)
    const to = [parts.join('/'), parts.join('/') + '.md'].find((p) => p.endsWith('.md') && exists(p))
    if (!to) return flash(tr('Note introuvable : {p}', { p: href }))
    app.current = to
    persist()
  }

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

  /** Embed vault files `rels` in `note` (if still the open one), at `pos` or the caret. */
  function embed(note: string, rels: string[], pos?: number) {
    if (note !== path || !rels.length) return // the user moved to another note meanwhile
    const dir = parent(note)
    const nodes = rels.map((rel) => ({ type: 'image', attrs: { src: encodeURI(relTo(dir, rel)), alt: rel.split('/').pop()!.replace(/\.\w+$/, '') } }))
    ;(pos == null ? cmd().insertContent(nodes) : cmd().insertContentAt(pos, nodes)).run()
  }

  /** Embed image files from disk: copied into the vault's `.ressources` first, unless already there. */
  async function embedPaths(files: string[], pos?: number) {
    const note = path
    if (!note || !app.vault) return
    const res = `${app.vault}/.ressources/`
    const rels: string[] = []
    for (const f of files)
      rels.push(f.startsWith(res) ? f.slice(app.vault.length + 1) : await call<string>('import_file', { src: f }))
    embed(note, rels, pos)
  }

  async function image() {
    const file = await open({ filters: [{ name: 'Images', extensions: IMAGES }] })
    if (typeof file === 'string') embedPaths([file])
  }

  /** Double click in the blank under a block: caret on the line after it. Under an image (a table, a formula…),
   *  which takes no text, that line is added if there is no empty one already. */
  function below(e: MouseEvent) {
    if (!editor || ![scroller, page, editor.view.dom].includes(e.target as HTMLElement)) return // a click in the text keeps its word selection
    const { view, state } = editor
    let after = -1
    state.doc.forEach((n, pos) => {
      const r = (view.nodeDOM(pos) as Element | null)?.getBoundingClientRect()
      if (r && r.bottom <= e.clientY) after = pos + n.nodeSize
    })
    if (after < 0) return
    const prev = state.doc.resolve(after).nodeBefore!, next = state.doc.nodeAt(after)
    const c = editor.chain().focus()
    if (prev.isTextblock) return void c.setTextSelection(after - 1).run()
    if (!(next?.type.name === 'paragraph' && !next.content.size)) c.insertContentAt(after, { type: 'paragraph' })
    c.setTextSelection(after + 1).run()
  }

  const imagePaths = (dt: DataTransfer | null) => diskPaths(dt).filter(isImage)

  /** Pasted or dropped images: written to the vault's `.ressources`, then embedded where they landed. */
  async function drop(files: File[], pos?: number) {
    const note = path
    if (!note) return
    const rels: string[] = []
    for (const f of files) {
      // a pasted screenshot has no real name
      const name = f.name && f.name !== 'image.png' ? f.name : `image-${new Date().toISOString().slice(0, 19).replace(/\D/g, '')}.${f.type.split('/')[1]?.replace('svg+xml', 'svg') ?? 'png'}`
      rels.push(await call<string>('save_file', new Uint8Array(await f.arrayBuffer()), {
        headers: { 'x-name': encodeURIComponent(name) },
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
  let lang = $state('') // the code block's language, for the toolbar's picker
  const LANGS = ['bash', 'c', 'cpp', 'csharp', 'css', 'diff', 'go', 'java', 'javascript', 'json', 'kotlin', 'lua', 'markdown', 'php', 'python', 'ruby', 'rust', 'sql', 'swift', 'typescript', 'xml', 'yaml']
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
    const md = raw ?? text()
    if (md === saved) return void (status = 'saved') // an update that changed nothing on disk (e.g. a node view settling)
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
      extensions: [...extensions, Markdown, keys, bubble(), files, wikiSuggest],
      content: body,
      contentType: 'markdown',
      // these run before plugins, so file:// images are taken here and real files fall through to FileHandler
      editorProps: {
        handlePaste: (_v, e) => {
          const p = imagePaths(e.clipboardData)
          if (p.length) embedPaths(p)
          return p.length > 0
        },
        handleClick: (_v, _pos, e) => {
          if (!(e.ctrlKey || e.metaKey)) return false
          const el = e.target as HTMLElement
          const wiki = el.closest('a[data-wiki]'), fn = el.closest('sup[data-fn]'), a = el.closest('a[href]')
          if (!wiki && !fn && !a) return false
          e.preventDefault()
          if (wiki) openWiki(wiki.getAttribute('data-wiki')!)
          else if (fn) toFootnote(fn.getAttribute('data-fn')!)
          else follow(a!.getAttribute('href')!)
          return true
        },
        handleDrop: (v, e) => {
          const p = imagePaths(e.dataTransfer)
          if (!p.length) return false
          e.preventDefault()
          embedPaths(p, v.posAtCoords({ left: e.clientX, top: e.clientY })?.pos)
          return true
        },
      },
      onSelectionUpdate: ({ editor: e }) => (picked = !e.state.selection.empty),
      onTransaction: () => {
        const a = tools.filter((t) => t?.active()).map((t) => t!.key)
        if (a.join() !== on.join()) on = a
        const u = !!editor?.can().undo(), re = !!editor?.can().redo()
        if (u !== can.undo || re !== can.redo) can = { undo: u, redo: re }
        lang = editor?.isActive('codeBlock') ? editor.getAttributes('codeBlock').language ?? '' : ''
        moveCaret()
      },
      onFocus: moveCaret,
      onBlur: moveCaret,
      onUpdate: ({ editor: e }) => {
        words = count(e.getText())
        raw = null
        src = text()
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
      // the asset protocol refuses any path with `..` in it (`../../.ressources/x.png`): resolve them here
      const parts: string[] = []
      for (const s of (p.startsWith('/') ? p : `${base}/${p}`).split('/'))
        if (s === '..') parts.pop()
        else if (s && s !== '.') parts.push(s)
      return convertFileSrc('/' + parts.join('/'))
    }
    make(body)
    src = disk
    raw = null
    status = 'saved'
    scroller?.scrollTo({ top: 0 })
    host.animate([{ opacity: 0, transform: 'translateY(6px)' }, { opacity: 1, transform: 'none' }], {
      duration: 220,
      easing: 'cubic-bezier(.2,.8,.2,1)',
    })
    if (app.fresh === next) {
      app.fresh = null
      requestAnimationFrame(() => titleEl?.select())
    } else if (document.activeElement?.closest('[role=tree]')) {
      // opened from the sidebar: focus stays there, so Delete / arrows act on the tree
    } else if (app.view === 'source') srcEl?.focus()
    else editor!.commands.focus('start')
  }

  async function rename(input: HTMLInputElement) {
    const name = input.value.trim().replace(/[\/]/g, '-')
    const from = app.current!
    const to = join(parent(from), name + '.md')
    if (!name || to === from) return void (input.value = title(from))
    if (exists(to)) {
      flash(tr('« {name} » existe déjà ici', { name }))
      return void (input.value = title(from))
    }
    await move(from, to)
  }

  $effect(() => {
    load(app.current)
  })

  // a keystroke in the source view: the rendered side follows when shown, the text is saved as typed
  function edit(v: string) {
    raw = src = v
    const [f, body] = split(v)
    front = f
    if (app.view === 'split') sync(body)
    status = 'saving'
    clearTimeout(timer)
    timer = window.setTimeout(save, 400)
  }
  function sync(body: string) {
    if (!editor) return
    editor.commands.setContent(body, { contentType: 'markdown', emitUpdate: false })
    words = count(editor.getText())
  }
  // back from the source view: the rendered side catches up with what was typed there
  $effect(() => {
    if (app.view !== 'source' && raw !== null) sync(split(raw)[1])
  })
  // undo / redo: the textarea's own history while it has the keyboard, else the editor's
  let can = $state({ undo: false, redo: false })
  const undoLabel = 'Annuler ({mod}+Z)', redoLabel = 'Rétablir ({mod}+Maj+Z)'
  function history(redo: boolean) {
    if (app.view === 'source' || document.activeElement === srcEl) {
      srcEl?.focus()
      document.execCommand(redo ? 'redo' : 'undo') // fires input, so edit() saves it
    } else if (redo) cmd().redo().run()
    else cmd().undo().run()
  }
  const views: { v: View; label: string; icon: typeof Eye }[] = [
    { v: 'rich', label: 'Rendu', icon: Eye },
    { v: 'source', label: 'Source Markdown', icon: FileCode },
    { v: 'split', label: 'Côte à côte', icon: Columns2 },
  ]

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
    const modKey = (e: KeyboardEvent) => document.documentElement.classList.toggle('mod-down', e.ctrlKey || e.metaKey)
    const modOff = () => document.documentElement.classList.remove('mod-down')
    window.addEventListener('keydown', modKey)
    window.addEventListener('keyup', modKey)
    window.addEventListener('blur', modOff)
    window.addEventListener('pointerup', up)
    host.addEventListener('mousemove', track)
    host.addEventListener('mouseleave', leave)
    // reflow (window resize, images loading) moves text under a still caret
    const ro = new ResizeObserver(() => moveCaret())
    ro.observe(host)
    const un = onDiskChange(async (paths) => {
      if (!path || !editor || !paths.includes(path)) return
      // background re-read: a failure (the note just moved or vanished) is not the user's concern
      const disk = await invoke<string>('read_note', { path }).catch(() => null)
      // only take the disk version when the user has nothing unsaved
      if (disk === null || disk === saved || (raw ?? text()) !== saved) return
      const [f, body] = split(disk)
      front = f
      const at = editor.state.selection.from
      editor.chain().setContent(body, { contentType: 'markdown', emitUpdate: false }).setTextSelection(Math.min(at, editor.state.doc.content.size)).run()
      saved = text()
      src = disk
      raw = null
      words = count(editor.getText())
      status = 'saved'
      ember++
    })
    return () => {
      window.removeEventListener('pointerup', up)
      window.removeEventListener('keydown', modKey)
      window.removeEventListener('keyup', modKey)
      window.removeEventListener('blur', modOff)
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
      <Tip class={on.includes(t.key) ? 'icon-btn on' : 'icon-btn'} aria-pressed={on.includes(t.key)} label={tr(t.label, { mod })} onclick={() => t.run()}><t.icon size={15} strokeWidth={1.6} /></Tip>
    {:else}
      <span class="sep"></span>
    {/if}
  {/each}
  {#if table && on.includes('codeBlock')}
    <span class="sep"></span>
    <select class="lang" aria-label={tr('Langage du bloc de code')} value={lang}
      onchange={(e) => cmd().updateAttributes('codeBlock', { language: e.currentTarget.value || null }).run()}>
      <option value="">{tr('Texte brut')}</option>
      {#each LANGS as l}<option value={l}>{l}</option>{/each}
      {#if lang && !LANGS.includes(lang)}<option value={lang}>{lang}</option>{/if}
    </select>
  {/if}
  {#if table && on.includes('table')}
    <span class="sep"></span>
    {#each tableTools as t, i}
      {#if i === 3 || i === 6}<span class="sep thin"></span>{/if}
      <Tip class={t.danger ? 'icon-btn danger' : 'icon-btn'} label={tr(t.label)} onclick={() => t.run(cmd()).run()}>{@render icon(t, 15)}</Tip>
    {/each}
  {/if}
{/snippet}

{#snippet icon(t: Item, size: number)}
  {#if typeof t.icon === 'string'}<TableIcon kind={t.icon} {size} />{:else}<t.icon {size} strokeWidth={1.6} />{/if}
{/snippet}

{#snippet tableMenu(k: 'row' | 'col', h: Hover, style: string)}
  <DropdownMenu.Root bind:open={() => menu === k, (o) => openMenu(k, o)}>
    <DropdownMenu.Trigger class={menu === k ? `grip ${k} open` : `grip ${k}`} aria-label={tr(k === 'row' ? 'Options de la ligne' : 'Options de la colonne')} {style}>
      {#if k === 'row'}<GripVertical size={12} />{:else}<GripHorizontal size={12} />{/if}
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content class="menu" side={k === 'row' ? 'left' : 'bottom'} align="start" sideOffset={4} collisionPadding={8} onCloseAutoFocus={(e) => e.preventDefault()}>
        {#each k === 'row' ? rowMenu : colMenu as it}
          <DropdownMenu.Item class={it.danger ? 'danger' : ''} onSelect={() => at(h.cell, it.run)}>{@render icon(it, 14)}{tr(it.label)}</DropdownMenu.Item>
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
{/snippet}


<section class="editor" class:empty={!app.current}>
  {#if app.current}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="toolbar" role="toolbar" aria-label={tr('Mise en forme')} tabindex="-1" onmousedown={(e) => e.preventDefault()}>
      {#if app.view !== 'source'}{@render buttons(true)}{/if}
      <div class="views">
        <Tip class="icon-btn" disabled={app.view === 'rich' && !can.undo} label={tr(undoLabel, { mod })} onclick={() => history(false)}><Undo2 size={15} strokeWidth={1.6} /></Tip>
        <Tip class="icon-btn" disabled={app.view === 'rich' && !can.redo} label={tr(redoLabel, { mod })} onclick={() => history(true)}><Redo2 size={15} strokeWidth={1.6} /></Tip>
        <span class="sep"></span>
        {#each views as { v, label, icon: I }}
          <Tip class={app.view === v ? 'icon-btn on' : 'icon-btn'} aria-pressed={app.view === v} label={tr(label) + ` (${mod}+${tr('Maj')}+M)`} onclick={() => setView(v)}><I size={15} strokeWidth={1.6} /></Tip>
        {/each}
      </div>
    </div>
  {/if}
  {#key ember}
    {#if ember}<div class="ember"></div>{/if}
  {/key}
  <div class="panes">
  {#if app.current && app.view !== 'rich'}
    <textarea class="source" bind:this={srcEl} value={src} spellcheck="false" aria-label={tr('Source Markdown')}
      oninput={(e) => edit(e.currentTarget.value)}></textarea>
  {/if}
  <div class="scroll" class:gone={app.view === 'source'} bind:this={scroller} role="presentation" ondblclick={below}>
    <div class="page" bind:this={page}>
    {#if app.current}
      <input
        class="title"
        bind:this={titleEl}
        value={title(app.current)}
        spellcheck="false"
        aria-label={tr('Titre de la note')}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === 'ArrowDown') { e.preventDefault(); editor?.commands.focus('start') }
          if (e.key === 'Escape') { e.currentTarget.value = title(app.current!); editor?.commands.focus() }
        }}
        onchange={(e) => rename(e.currentTarget)}
      />
    {/if}
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        {#snippet child({ props })}<div {...props} bind:this={host} oncontextmenucapture={aim}></div>{/snippet}
      </ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content class="menu" collisionPadding={8} onCloseAutoFocus={(e) => { e.preventDefault(); editor?.commands.focus() }}>
          <ContextMenu.Item disabled={!can.undo} onSelect={() => history(false)}><Undo2 size={14} strokeWidth={1.6} />{tr(undoLabel, { mod }).replace(/ \(.*/, '')}<kbd>{mod}+Z</kbd></ContextMenu.Item>
          <ContextMenu.Item disabled={!can.redo} onSelect={() => history(true)}><Redo2 size={14} strokeWidth={1.6} />{tr(redoLabel, { mod }).replace(/ \(.*/, '')}<kbd>{mod}+{tr('Maj')}+Z</kbd></ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item disabled={!picked} onSelect={() => clip('cut')}><Scissors size={14} strokeWidth={1.6} />{tr('Couper')}<kbd>{mod}+X</kbd></ContextMenu.Item>
          <ContextMenu.Item disabled={!picked} onSelect={() => clip('copy')}><Copy size={14} strokeWidth={1.6} />{tr('Copier')}<kbd>{mod}+C</kbd></ContextMenu.Item>
          <ContextMenu.Item onSelect={pasteClip}><ClipboardPaste size={14} strokeWidth={1.6} />{tr('Coller')}<kbd>{mod}+V</kbd></ContextMenu.Item>
          {#if picked}
            <ContextMenu.Separator />
            {#each ['bold', 'italic', 'strike', 'code'].map(tool) as t}
              <ContextMenu.Item class={on.includes(t.key) ? 'on' : ''} onSelect={() => t.run()}><t.icon size={14} strokeWidth={1.6} />{tr(t.label, { mod }).replace(/ \(.*/, '')}<kbd>{tr(t.label, { mod }).match(/\((.*)\)/)?.[1]}</kbd></ContextMenu.Item>
            {/each}
          {/if}
          <ContextMenu.Separator />
          {#if on.includes('link')}
            <ContextMenu.Item onSelect={() => follow(editor!.getAttributes('link').href)}><ExternalLink size={14} strokeWidth={1.6} />{tr('Ouvrir le lien')}<kbd>{mod}+{tr('clic')}</kbd></ContextMenu.Item>
            <ContextMenu.Item onSelect={editLink}><Link size={14} strokeWidth={1.6} />{tr('Modifier le lien')}<kbd>{mod}+L</kbd></ContextMenu.Item>
            <ContextMenu.Item onSelect={() => cmd().extendMarkRange('link').unsetLink().run()}><Unlink size={14} strokeWidth={1.6} />{tr('Retirer le lien')}</ContextMenu.Item>
          {:else}
            <ContextMenu.Item onSelect={editLink}><Link size={14} strokeWidth={1.6} />{tr('Insérer un lien')}<kbd>{mod}+L</kbd></ContextMenu.Item>
          {/if}
          <ContextMenu.Separator />
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger><Pilcrow size={14} strokeWidth={1.6} />{tr('Transformer en')}<ChevronRight class="sub" size={13} /></ContextMenu.SubTrigger>
            <ContextMenu.SubContent class="menu" collisionPadding={8}>
              <ContextMenu.Item class={on.some((k) => blocks.some((b) => b.key === k)) ? '' : 'on'} onSelect={() => cmd().setParagraph().run()}><Pilcrow size={14} strokeWidth={1.6} />{tr('Texte')}</ContextMenu.Item>
              {#each blocks as t}
                <ContextMenu.Item class={on.includes(t.key) ? 'on' : ''} onSelect={() => t.run()}><t.icon size={14} strokeWidth={1.6} />{tr(t.label)}</ContextMenu.Item>
              {/each}
            </ContextMenu.SubContent>
          </ContextMenu.Sub>
          {#if on.includes('table')}
            <ContextMenu.Sub>
              <ContextMenu.SubTrigger><Table size={14} strokeWidth={1.6} />{tr('Tableau')}<ChevronRight class="sub" size={13} /></ContextMenu.SubTrigger>
              <ContextMenu.SubContent class="menu" collisionPadding={8}>
                {#each tableTools as t, i}
                  {#if i === 3 || i === 6}<ContextMenu.Separator />{/if}
                  <ContextMenu.Item class={t.danger ? 'danger' : ''} onSelect={() => t.run(cmd()).run()}>{@render icon(t, 14)}{tr(t.label)}</ContextMenu.Item>
                {/each}
              </ContextMenu.SubContent>
            </ContextMenu.Sub>
          {:else}
            <ContextMenu.Item onSelect={() => tool('table').run()}><Table size={14} strokeWidth={1.6} />{tr('Insérer un tableau')}</ContextMenu.Item>
          {/if}
          <ContextMenu.Item onSelect={() => tool('image').run()}><Image size={14} strokeWidth={1.6} />{tr('Insérer une image')}</ContextMenu.Item>
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger><Plus size={14} strokeWidth={1.6} />{tr('Insérer')}<ChevronRight class="sub" size={13} /></ContextMenu.SubTrigger>
            <ContextMenu.SubContent class="menu" collisionPadding={8}>
              <ContextMenu.Item onSelect={() => cmd().insertContent('[[').run()}><FileText size={14} strokeWidth={1.6} />{tr('Lien vers une note')}<kbd>[[</kbd></ContextMenu.Item>
              <ContextMenu.Item onSelect={() => newMath(false)}><Sigma size={14} strokeWidth={1.6} />{tr('Formule')}<kbd>$…$</kbd></ContextMenu.Item>
              <ContextMenu.Item onSelect={() => newMath(true)}><SquareSigma size={14} strokeWidth={1.6} />{tr('Formule centrée')}<kbd>$$…$$</kbd></ContextMenu.Item>
              <ContextMenu.Item onSelect={footnote}><Superscript size={14} strokeWidth={1.6} />{tr('Note de bas de page')}<kbd>[^1]</kbd></ContextMenu.Item>
              <ContextMenu.Item onSelect={() => cmd().insertContent([{ type: 'blockquote', content: [{ type: 'paragraph', content: [{ type: 'text', text: '[!note] ' }] }] }]).run()}><TextQuote size={14} strokeWidth={1.6} />{tr('Encadré')}<kbd>&gt; [!note]</kbd></ContextMenu.Item>
              <ContextMenu.Item onSelect={() => tool('hr').run()}><Minus size={14} strokeWidth={1.6} />{tr('Séparateur')}<kbd>---</kbd></ContextMenu.Item>
            </ContextMenu.SubContent>
          </ContextMenu.Sub>
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={() => cmd().selectAll().run()}><TextSelect size={14} strokeWidth={1.6} />{tr('Tout sélectionner')}<kbd>{mod}+A</kbd></ContextMenu.Item>
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
    {#if field}
      {@const f = field}
      <input class="linkfield" style:left="{f.x}px" style:top="{f.y}px" value={f.value} spellcheck="false"
        {@attach (el) => { requestAnimationFrame(() => (el.focus(), el.select())) }}
        placeholder={f.hint} aria-label={f.label}
        onkeydown={(e) => {
          // read f before clearing: it is derived from `field`
          if (e.key === 'Enter') { e.preventDefault(); const { apply } = f, v = e.currentTarget.value; field = null; apply(v) }
          if (e.key === 'Escape') { e.preventDefault(); field = null; editor?.commands.focus() }
        }}
        onblur={() => (field = null)} />
    {/if}
    {#if sug}
      <div class="menu sug" role="listbox" style:left="{sug.x}px" style:top="{sug.y}px">
        {#each sug.items as it, i}
          <button role="option" aria-selected={i === sug.index} class:on={i === sug.index} onmousedown={(e) => { e.preventDefault(); sug!.pick(it) }}>
            <FileText size={13} strokeWidth={1.6} /><span>{title(it)}</span><small>{parent(it)}</small>
          </button>
        {/each}
      </div>
    {/if}
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
        <Tip class="add" label={tr('Ajouter une ligne')} style="left:{h.tx}px;top:{h.ty + h.th + 4}px;width:{h.tw}px;height:14px" onclick={() => at(lastRow(h.table), (c) => c.addRowAfter())}><Plus size={11} /></Tip>
        <Tip class="add" label={tr('Ajouter une colonne')} style="left:{h.tx + h.tw + 4}px;top:{h.ty}px;width:14px;height:{h.th}px" onclick={() => at(lastCol(h.table), (c) => c.addColumnAfter())}><Plus size={11} /></Tip>
      </div>
    {/if}
    </div>
  </div>
  </div>

  <!-- shown, hidden and placed by the bubble menu extension -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="bar" role="toolbar" aria-label={tr('Mise en forme de la sélection')} tabindex="-1" bind:this={barEl} style="visibility: hidden" onmousedown={(e) => e.preventDefault()}>
    {@render buttons(false)}
  </div>

  {#if app.current}
    <footer>
      <span>{tr(words === 1 ? '{n} mot' : '{n} mots', { n: words })}</span>
      <span class="state" class:busy={status === 'saving'}>{tr(status === 'saved' ? 'Enregistré' : 'Enregistrement')}</span>
    </footer>
  {:else}
    <div class="blank">
      <p>{tr('Aucune note ouverte')}</p>
      <dl>
        <div><dt>{tr('Ouvrir une note')}</dt><dd><kbd>{mod}</kbd><kbd>P</kbd></dd></div>
        <div><dt>{tr('Nouvelle note')}</dt><dd><kbd>{mod}</kbd><kbd>N</kbd></dd></div>
        <div><dt>{tr('Commandes')}</dt><dd><kbd>{mod}</kbd><kbd>K</kbd></dd></div>
        <div><dt>{tr('Agent')}</dt><dd><kbd>{mod}</kbd><kbd>J</kbd></dd></div>
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
    padding: 56px clamp(16px, 5%, 32px) 30vh;
  }
  .empty .scroll, .scroll.gone { display: none; }
  .panes { flex: 1; min-height: 0; display: flex; }
  .scroll { min-width: 0; }
  .source {
    flex: 1;
    min-width: 0;
    resize: none;
    border: 0;
    outline: none;
    padding: 32px clamp(16px, 4%, 32px) 30vh;
    background: var(--bg);
    color: var(--fg);
    font: 13.5px/1.65 var(--mono);
    tab-size: 2;
  }
  .source + .scroll { border-left: 1px solid var(--line-soft); }

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

  .page { position: relative; max-width: var(--page-w); margin: 0 auto; }
  /* layout options (Settings) */
  :global(:root[data-page='left']) .page { margin-left: 24px; }
  :global(:root[data-toolbar='left']) .toolbar > :global(:first-child) { margin-left: 4px; }

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
    gap: 1px;
    height: 40px;
    padding: 0 8px;
    border-bottom: 1px solid var(--line-soft);
    /* too narrow: scroll sideways; centred with auto margins so the first buttons stay reachable */
    overflow-x: auto;
    scrollbar-width: none;
  }
  .toolbar > :global(:first-child) { margin-left: auto; }
  .toolbar > :global(:last-child) { margin-right: auto; }
  .toolbar :global(.icon-btn) { flex: none; }
  .toolbar .sep { flex: none; }
  /* view switch pinned right (stays put when the mode hides the formatting buttons), visible when the bar scrolls */
  .toolbar > .views.views { flex: none; display: flex; align-items: center; gap: 1px; margin: 0 0 0 auto; padding-left: 8px; position: sticky; right: 0; background: var(--bg); }
  .bar :global(.icon-btn), .toolbar :global(.icon-btn) { width: 28px; height: 28px; color: var(--muted); }
  .bar :global(.icon-btn.on), .toolbar :global(.icon-btn.on) { color: var(--orange); background: color-mix(in srgb, var(--orange) 12%, transparent); }
  .bar .sep, .toolbar .sep { width: 1px; height: 16px; margin: 0 3px; background: var(--line-soft); }
  .toolbar .sep.thin { height: 8px; margin: 0 1px; }
  .toolbar :global(.icon-btn.danger:hover) { color: var(--orange); }
  @keyframes bar { from { opacity: 0; } }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 16px;
    white-space: nowrap;
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
    font: 400 var(--doc-size)/var(--doc-line) var(--text);
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

  :global(.tiptap mark) { color: inherit; background: color-mix(in srgb, var(--amber) 30%, transparent); border-radius: 2px; padding: 0 1px; }
  /* [[wiki links]] and footnote refs: atoms, opened with Ctrl/Cmd+click */
  :global(.tiptap a.wiki) { text-decoration: none; border-bottom: 1px dashed color-mix(in srgb, var(--orange) 50%, transparent); }
  :global(.tiptap a.wiki::before) { content: '[['; opacity: .35; }
  :global(.tiptap a.wiki::after) { content: ']]'; opacity: .35; }
  :global(.tiptap a.wiki.ProseMirror-selectednode), :global(.tiptap sup.fn.ProseMirror-selectednode) { background: var(--select); border-radius: 3px; }
  :global(.tiptap sup.fn) { color: var(--orange); font: 600 .72em var(--display); padding: 0 1px; }
  :global(.mod-down .tiptap :is(a.wiki, sup.fn)) { cursor: pointer; }
  :global(.tiptap .fn-def) { display: flex; gap: 8px; font-size: .9em; color: var(--muted); }
  :global(.tiptap .fn-def:first-of-type) { border-top: 1px solid var(--line-soft); padding-top: .8em; margin-top: 1.4em; }
  :global(.tiptap .fn-id) { flex: none; color: var(--orange); font: 600 .8em/2 var(--display); }
  :global(.tiptap .fn-id::after) { content: '.'; }
  /* > [!kind] callouts */
  :global(.tiptap blockquote.callout) { --c: var(--orange); border-left-color: var(--c); background: color-mix(in srgb, var(--c) 7%, transparent); border-radius: 0 6px 6px 0; padding: .55em .9em; color: var(--fg); }
  :global(.tiptap blockquote.callout:is([data-callout='warning'], [data-callout='caution'], [data-callout='attention'])) { --c: var(--amber); }
  :global(.tiptap blockquote.callout:is([data-callout='danger'], [data-callout='error'], [data-callout='bug'], [data-callout='failure'])) { --c: #e5484d; }
  :global(.tiptap blockquote.callout:is([data-callout='tip'], [data-callout='success'], [data-callout='check'], [data-callout='done'])) { --c: #3fa66a; }
  :global(.tiptap blockquote.callout:is([data-callout='note'], [data-callout='info'], [data-callout='todo'], [data-callout='abstract'], [data-callout='question'])) { --c: #4d8fdb; }
  :global(.tiptap .callout-title) { font-weight: 640; }
  :global(.tiptap .callout-kind) { color: var(--c); font: 600 .78em var(--mono); letter-spacing: .02em; }
  /* math (KaTeX): click to edit */
  :global(.tiptap :is(.tiptap-mathematics-render, [data-type='inline-math'], [data-type='block-math'])) { cursor: pointer; border-radius: 4px; }
  :global(.tiptap :is([data-type='inline-math'], [data-type='block-math']):hover) { background: var(--hover); }
  :global(.tiptap [data-type='block-math']) { display: block; text-align: center; padding: .4em 0; }
  /* syntax highlighting (lowlight / highlight.js classes), from the theme's own colours */
  :global(.tiptap pre :is(.hljs-keyword, .hljs-selector-tag, .hljs-built_in, .hljs-doctag)) { color: var(--orange); }
  :global(.tiptap pre :is(.hljs-string, .hljs-regexp, .hljs-addition, .hljs-attribute)) { color: #3fa66a; }
  :global(.tiptap pre :is(.hljs-number, .hljs-literal, .hljs-symbol, .hljs-variable.language_)) { color: #4d8fdb; }
  :global(.tiptap pre :is(.hljs-title, .hljs-section, .hljs-type, .hljs-name, .hljs-selector-class)) { color: var(--amber); }
  :global(.tiptap pre :is(.hljs-comment, .hljs-quote, .hljs-meta)) { color: var(--faint); font-style: italic; }
  :global(.tiptap pre .hljs-deletion) { color: #e5484d; }
  :global(.tiptap pre :is(.hljs-attr, .hljs-property, .hljs-params)) { color: var(--muted); }

  .lang {
    height: 26px;
    padding: 0 6px;
    font: 500 12px var(--mono);
    color: var(--muted);
    background: transparent;
    border: 1px solid var(--line-soft);
    border-radius: 5px;
  }
  .sug { position: absolute; min-width: 240px; max-width: 360px; }
  .sug button {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border-radius: 5px;
    font: 500 13px/1.2 var(--display);
    color: var(--fg);
    cursor: pointer;
  }
  .sug button :global(svg) { flex: none; color: var(--muted); }
  .sug button span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sug button small { margin-left: auto; padding-left: 12px; color: var(--faint); font-size: 11.5px; white-space: nowrap; }
  .sug button.on { background: var(--orange-wash); }

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

  /* Ctrl/Cmd held: links show they can be opened */
  :global(.mod-down .tiptap a) { cursor: pointer; }
  .linkfield {
    position: absolute;
    z-index: 60;
    width: min(340px, 80%);
    padding: 7px 10px;
    font: 500 13px/1.2 var(--mono, monospace);
    color: var(--fg);
    background: var(--surface-solid);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 8px 24px -10px #00000066;
    outline: none;
  }
  .linkfield:focus { border-color: var(--orange); }
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
