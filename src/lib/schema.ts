// The editor's document model. Markdown is only the file format: parsed on load, serialized on save.
import StarterKit from '@tiptap/starter-kit'
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

export const extensions = [
  StarterKit.configure({ link: { openOnClick: false }, underline: false }),
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
  return t.replace(/[\\`*_~<&]/g, (c, i: number) => {
    const a = t[i - 1] ?? ' ', b = t[i + 1] ?? ' '
    if (c === '_' && word.test(a) && word.test(b)) return c // intraword: never emphasis
    if (c === '*' && /\s/.test(a) && /\s/.test(b)) return c // "3 * 4"
    if (c === '<' && !/[A-Za-z/!?]/.test(b)) return c // not a tag
    if (c === '&' && !/^&(#\d+|#x[\da-f]+|\w+);/i.test(t.slice(i))) return c // not an entity
    return '\\' + c
  })
}
const proto = MarkdownManager.prototype as any
if (typeof proto.encodeTextForMarkdown !== 'function') throw new Error('@tiptap/markdown: encodeTextForMarkdown is gone, update schema.ts')
proto.encodeTextForMarkdown = function (text: string, node: any, parent: any) {
  const code = (parent?.type && this.codeTypes.has(parent.type)) || (node.marks ?? []).some((m: any) => this.codeTypes.has(typeof m === 'string' ? m : m.type))
  return code ? text : escape(text)
}
