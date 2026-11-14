// The editor's document model. Markdown is only the file format: parsed on load, serialized on save.
import { Node, Extension, mergeAttributes, InputRule } from '@tiptap/core'
import { Plugin } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import StarterKit from '@tiptap/starter-kit'
import Highlight from '@tiptap/extension-highlight'
import Link from '@tiptap/extension-link'
import { InlineMath, BlockMath } from '@tiptap/extension-mathematics'
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import { createLowlight, common } from 'lowlight'
import { TaskList, TaskItem } from '@tiptap/extension-list'
import { TableKit } from '@tiptap/extension-table'
import Image from '@tiptap/extension-image'
import { MarkdownManager } from '@tiptap/markdown'

/** How the editor turns an image's markdown src into a URL the webview can load (set per note). */
export const media = { resolve: (src: string) => src }

const Img = Image.extend({
  renderHTML({ HTMLAttributes }) {
    return ['img', { ...HTMLAttributes, src: media.resolve(HTMLAttributes.src ?? '') }]
  },
})

/** Editor-side hooks the schema calls into (the markdown check runs without an editor). */
export const ui = { math: (_latex: string, _pos: number, _block: boolean) => {} }

/** `[[note]]`, `[[folder/note|shown text]]`, `[[note#heading]]`: an Obsidian-style link to another note. */
export const WikiLink = Node.create({
  name: 'wikiLink',
  group: 'inline',
  inline: true,
  atom: true,
  addAttributes: () => ({ target: { default: '' }, alias: { default: null } }),
  parseHTML: () => [{ tag: 'a[data-wiki]', getAttrs: (el) => ({ target: el.getAttribute('data-wiki'), alias: el.getAttribute('data-alias') }) }],
  renderHTML: ({ node }) => ['a', { 'data-wiki': node.attrs.target, 'data-alias': node.attrs.alias, class: 'wiki', href: '#' }, node.attrs.alias || node.attrs.target],
  renderText: ({ node }) => node.attrs.alias || node.attrs.target,
  // typing the closing ]] by hand (the suggestion list covers the usual way)
  addInputRules() {
    return [new InputRule({
      find: /\[\[([^\[\]|\n]+)(?:\|([^\[\]\n]+))?\]\]$/,
      handler: ({ state, range, match }) => {
        state.tr.replaceWith(range.from, range.to, this.type.create({ target: match[1].trim(), alias: match[2]?.trim() || null }))
      },
    })]
  },
  markdownTokenizer: {
    name: 'wikiLink',
    level: 'inline',
    start: (src: string) => src.indexOf('[['),
    tokenize(src: string) {
      const m = /^\[\[([^\[\]|\n]+)(?:\|([^\[\]\n]+))?\]\]/.exec(src)
      if (m) return { type: 'wikiLink', raw: m[0], target: m[1].trim(), alias: m[2]?.trim() || null }
    },
  },
  parseMarkdown: (t: any, h: any) => h.createNode('wikiLink', { target: t.target, alias: t.alias }),
  renderMarkdown: (n: any) => `[[${n.attrs.target}${n.attrs.alias ? '|' + n.attrs.alias : ''}]]`,
})

/** `[^id]` in the text… */
const FootnoteRef = Node.create({
  name: 'footnoteRef',
  group: 'inline',
  inline: true,
  atom: true,
  addAttributes: () => ({ id: { default: '1' } }),
  parseHTML: () => [{ tag: 'sup[data-fn]', getAttrs: (el) => ({ id: el.getAttribute('data-fn') }) }],
  renderHTML: ({ node }) => ['sup', { 'data-fn': node.attrs.id, class: 'fn' }, node.attrs.id],
  markdownTokenizer: {
    name: 'footnoteRef',
    level: 'inline',
    start: (src: string) => src.indexOf('[^'),
    tokenize(src: string) {
      const m = /^\[\^([^\]\s]+)\](?!:)/.exec(src)
      if (m) return { type: 'footnoteRef', raw: m[0], id: m[1] }
    },
  },
  parseMarkdown: (t: any, h: any) => h.createNode('footnoteRef', { id: t.id }),
  renderMarkdown: (n: any) => `[^${n.attrs.id}]`,
})

/** …and its `[^id]: text` definition, a block of its own (marked would otherwise read some as link definitions and drop them) */
const Footnote = Node.create({
  name: 'footnote',
  group: 'block',
  content: 'inline*',
  defining: true,
  addAttributes: () => ({ id: { default: '1' } }),
  parseHTML: () => [{ tag: 'div[data-fn-def]', getAttrs: (el) => ({ id: el.getAttribute('data-fn-def') }) }],
  renderHTML: ({ node, HTMLAttributes }) => ['div', mergeAttributes(HTMLAttributes, { 'data-fn-def': node.attrs.id, class: 'fn-def' }), ['span', { class: 'fn-id', contenteditable: 'false' }, node.attrs.id], ['span', 0]],
  markdownTokenizer: {
    name: 'footnote',
    level: 'block',
    // only at a line start: anywhere else would cut the paragraph holding a [^ref]
    start: (src: string) => /^\[\^[^\]\s]+\]:/m.exec(src)?.index ?? -1,
    tokenize(src: string, _tokens: unknown, h: any) {
      const m = /^\[\^([^\]\s]+)\]:[ \t]*([^\n]*)(?:\n|$)/.exec(src)
      if (m) return { type: 'footnote', raw: m[0], id: m[1], tokens: h.inlineTokens(m[2]) }
    },
  },
  parseMarkdown: (t: any, h: any) => h.createNode('footnote', { id: t.id }, h.parseInline(t.tokens ?? [])),
  renderMarkdown: (n: any, h: any) => `[^${n.attrs.id}]: ${h.renderChildren(n.content ?? [])}`,
})

// `$…$` the pandoc way: no space just inside the dollars, no digit right after, so "$5 and $10" stays text
const Inline$ = InlineMath.extend({
  markdownTokenizer: {
    name: 'inlineMath',
    level: 'inline',
    start: (src: string) => src.indexOf('$'),
    tokenize(src: string) {
      const m = /^\$(?![\s$])([^$\n]*?[^\s\\$])\$(?![\d$])/.exec(src) ?? /^\$([^\s$\\])\$(?![\d$])/.exec(src)
      if (m) return { type: 'inlineMath', raw: m[0], latex: m[1] }
    },
  },
}).configure({ onClick: (n, pos) => ui.math(n.attrs.latex, pos, false), katexOptions: { throwOnError: false } })
const Block$ = BlockMath.configure({ onClick: (n, pos) => ui.math(n.attrs.latex, pos, true), katexOptions: { throwOnError: false, displayMode: true } })

/** `> [!note] Title` blockquotes (Obsidian / GitHub callouts): stay plain blockquotes in the file, styled here. */
const Callouts = Extension.create({
  name: 'callouts',
  addProseMirrorPlugins: () => [
    new Plugin({
      props: {
        decorations(state) {
          const out: Decoration[] = []
          state.doc.descendants((node, pos) => {
            if (node.type.name !== 'blockquote') return true
            const first = node.firstChild
            const m = first?.type.name === 'paragraph' && /^\[!(\w+)\][+-]?/.exec(first.textContent)
            if (m) {
              // the first line (up to the soft break) is the title
              const nl = first!.textContent.indexOf('\n'), end = pos + 2 + (nl < 0 ? first!.content.size : nl)
              out.push(Decoration.node(pos, pos + node.nodeSize, { class: 'callout', 'data-callout': m[1].toLowerCase() }))
              out.push(Decoration.inline(pos + 2, end, { class: 'callout-title' }))
              out.push(Decoration.inline(pos + 2, pos + 2 + m[0].length, { class: 'callout-kind' }))
            }
            return true
          })
          return DecorationSet.create(state.doc, out)
        },
      },
    }),
  ],
})

// a bare URL in the file (read as a link by GFM) is written back bare, not as [url](url)
const BareLink = Link.extend({
  // marks are rendered around a placeholder; the real text comes in meta.markText
  renderMarkdown: (node: any, h: any, ctx: any) => {
    const href = node.attrs?.href ?? '', title = node.attrs?.title, text = h.renderChildren(node), shown = ctx?.meta?.markText ?? text
    if (!title && (shown === href || 'mailto:' + shown === href)) return text
    return title ? `[${text}](${href} "${title}")` : `[${text}](${href})`
  },
}).configure({ openOnClick: false })

export const lowlight = createLowlight(common)

export const extensions = [
  StarterKit.configure({ link: false, underline: false, codeBlock: false }),
  BareLink,
  CodeBlockLowlight.configure({ lowlight, defaultLanguage: null }),
  Highlight,
  Inline$,
  Block$,
  WikiLink,
  FootnoteRef,
  Footnote,
  Callouts,
  TaskList,
  TaskItem.configure({ nested: true }),
  TableKit.configure({ table: { resizable: false } }),
  Img,
]

/** Frontmatter isn't editable content: kept verbatim around the editor. */
export function split(text: string): [front: string, body: string] {
  const m = /^---\r?\n[\s\S]*?\r?\n---\r?\n?/.exec(text)
  return m ? [m[0], text.slice(m[0].length)] : ['', text]
}

// The stock serializer escapes every * _ [ ] ~ and turns < & > into entities, so plain text like
// "a < b" or "file_name" lands on disk as "a &lt; b", "file\_name". Escape only what would really
// be read back as syntax.
// ponytail: patches a private method of @tiptap/markdown; checks/markdown.check.ts catches it if an upgrade renames it
const word = /[\p{L}\p{N}]/u
export function escape(t: string) {
  return t.replace(/[\\`*_~<&$]/g, (c, i: number) => {
    const a = t[i - 1] ?? ' ', b = t[i + 1] ?? ' '
    if (c === '_' && word.test(a) && word.test(b)) return c // intraword: never emphasis
    if (c === '*' && /\s/.test(a) && /\s/.test(b)) return c // "3 * 4"
    if (c === '<' && !/[A-Za-z/!?]/.test(b)) return c // not a tag
    if (c === '&' && !/^&(#\d+|#x[\da-f]+|\w+);/i.test(t.slice(i))) return c // not an entity
    if (c === '$' && (/[\s\d]/.test(b) || !t.slice(i + 1).includes('$'))) return c // "$5", a lone $: not math
    return '\\' + c
  })
}
const proto = MarkdownManager.prototype as any
if (typeof proto.encodeTextForMarkdown !== 'function') throw new Error('@tiptap/markdown: encodeTextForMarkdown is gone, update schema.ts')
proto.encodeTextForMarkdown = function (text: string, node: any, parent: any) {
  const code = (parent?.type && this.codeTypes.has(parent.type)) || (node.marks ?? []).some((m: any) => this.codeTypes.has(typeof m === 'string' ? m : m.type))
  return code ? text : escape(text)
}
