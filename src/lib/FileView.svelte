<script lang="ts">
  // A file of the vault that is not a note: images shown, text edited as is (no formatting tools),
  // anything else handed to the system's default app.
  import { onMount } from 'svelte'
  import { convertFileSrc, invoke } from '@tauri-apps/api/core'
  import { ExternalLink } from '@lucide/svelte'
  import { t } from './i18n.svelte'
  import { app, call, hooks, onDiskChange, isImage } from './store.svelte'

  let path: string | null = null
  let text = $state<string | null>(null) // null: not text (or not read yet)
  let saved = ''
  let status = $state<'saved' | 'saving'>('saved')
  let timer = 0
  const image = $derived(!!app.current && isImage(app.current))
  const name = $derived(app.current?.split('/').pop() ?? '')

  async function save() {
    clearTimeout(timer)
    if (!path || text === null || text === saved) return void (status = 'saved')
    const p = path, v = text
    await call('write_file', { path: p, content: v })
    saved = v
    status = 'saved'
  }
  hooks.flush = save

  async function load(next: string | null) {
    await save()
    path = next
    text = null
    if (!next || isImage(next)) return
    const disk = await invoke<string>('read_file', { path: next }).catch(() => null)
    if (path !== next) return
    saved = disk ?? ''
    text = disk
    status = 'saved'
  }
  $effect(() => {
    load(app.current)
  })

  function edit(v: string) {
    text = v
    status = 'saving'
    clearTimeout(timer)
    timer = window.setTimeout(save, 400)
  }

  onMount(() => {
    const un = onDiskChange(async (paths) => {
      if (!path || text === null || !paths.includes(path) || text !== saved) return // unsaved typing wins
      const disk = await invoke<string>('read_file', { path }).catch(() => null)
      if (disk !== null && text === saved) text = saved = disk
    })
    return () => {
      save()
      if (hooks.flush === save) hooks.flush = async () => {}
      un.then((f) => f())
    }
  })
</script>

<section class="file">
  <h1 class="name">{name}</h1>
  {#if image}
    <div class="view"><img src={convertFileSrc(`${app.vault}/${app.current}`)} alt={name} /></div>
  {:else if text !== null}
    <textarea class="text" value={text} spellcheck="false" aria-label={name} oninput={(e) => edit(e.currentTarget.value)}></textarea>
    <footer><span class="state" class:busy={status === 'saving'}>{t(status === 'saved' ? 'Enregistré' : 'Enregistrement')}</span></footer>
  {:else}
    <div class="view none">
      <p>{t('Aperçu indisponible pour ce fichier')}</p>
      <button class="open" onclick={() => call('open_external', { path: app.current })}>
        <ExternalLink size={14} strokeWidth={1.6} />{t("Ouvrir avec l'application par défaut")}
      </button>
    </div>
  {/if}
</section>

<style>
  .file {
    height: 100%;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .name {
    margin: 0;
    padding: 20px clamp(16px, 4%, 32px) 14px;
    border-bottom: 1px solid var(--line-soft);
    font: 650 18px/1.2 var(--display);
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text {
    flex: 1;
    min-height: 0;
    resize: none;
    border: 0;
    outline: none;
    padding: 24px clamp(16px, 4%, 32px) 30vh;
    background: var(--bg);
    color: var(--fg);
    font: 13.5px/1.65 var(--mono);
    tab-size: 2;
  }
  .view {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    overflow: auto;
  }
  .view img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .none { place-content: center; gap: 16px; color: var(--muted); }
  .none p { margin: 0; font: 600 15px/1 var(--display); }
  .open {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border-radius: var(--r);
    border: 1px solid var(--line);
    font: 600 13px/1.2 var(--display);
    color: var(--fg);
    transition: background var(--t-fast);
  }
  .open:hover { background: var(--hover); }
  footer {
    display: flex;
    justify-content: flex-end;
    padding: 6px 16px 8px;
    font: 500 11.5px/1 var(--display);
    color: var(--faint);
  }
  .state { transition: color var(--t) var(--ease); }
  .state.busy { color: var(--muted); }
</style>
