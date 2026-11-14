// run: node --experimental-strip-types checks/markdown.check.ts [file.md ...]
// Load → save round trip through the editor's markdown pipeline. With files: prints each result
// (use it on a real vault before trusting the editor with it). Without: asserts on a sample.
import assert from 'node:assert'
import { readFileSync } from 'node:fs'
import { MarkdownManager } from '@tiptap/markdown'
import { extensions, split, escape } from '../src/lib/schema.ts'

const md = new MarkdownManager({ extensions })
const trip = (s: string) => { const [front, body] = split(s); return front + md.serialize(md.parse(body)) }

const files = process.argv.slice(2)
if (files.length) {
  for (const f of files) {
    const text = readFileSync(f, 'utf8'), once = trip(text)
    console.log(`===== ${f}${once.trim() === text.trim() ? ' (identical)' : `\n${once}`}`)
  }
  process.exit(0)
}

const kept = [
  '---\ntags: [a, b]\n---\n# Title',
  'Some **bold**, *italic*, ~~strike~~, `code` and a [link](https://x.y).',
  '> quote\n> two',
  '- a\n  - nested\n- b',
  '1. one\n2. two',
  '- [ ] todo\n- [x] done',
  '```python\nprint("hi")\n```',
  '![img](pic.png)',
  'a < b & c > d, l\'été, 50% — ok. file_name.md, [[other note]], 3 * 4.',
  'a\n\n\n\nb',
  'See [[other note]], [[dir/note|alias]] and ==marked== text.',
  'Euler: $e^{i\\pi} + 1 = 0$, but $5 and $10 stay text.',
  '$$\nx^2\n$$',
  'A claim[^1] and another[^note].',
  '[^1]: The source, see https://x.y.',
  '[^note]: Second one.',
  '> [!warning] Careful\n> body',
  '1. one\n2. two\n\n---\n\nafter',
  '```js\nconst a = 1\n```',
]
const sample = kept.join('\n\n')
const out = trip(sample)
for (const k of kept) assert.ok(out.includes(k), `lost on round trip:\n${k}\n--- got ---\n${out}`)
assert.match(trip('| a | b |\n|---|---|\n| 1 | 2 |'), /\| a +\| b +\|\n\| -+ \| -+ \|\n\| 1 +\| 2 +\|/)
assert.equal(trip(out), out, 'not stable on a second trip')

// literal syntax characters still round-trip as text, not formatting
for (const t of ['*not em*', '_x_', '`tick`', '<b>', '&amp;', '~~no~~', 'a\\b', '$x$', 'costs $5', '$'])
  assert.equal(md.parse(escape(t)).content?.[0]?.content?.[0]?.text, t, `escape(${t})`)
console.log('markdown ok')
