<script lang="ts">
  import { app, resolve, abortMerge } from './store.svelte'
  import { t } from './i18n.svelte'
</script>

<main class="conflicts">
  <h1>{t('Conflits de synchronisation')}</h1>
  <p>
    {t("Ces fichiers ont changé ici et sur le dépôt distant. Les notes restent fermées tant que chacun n'est pas résolu.")}
    {t('Pour fusionner à la main, éditez les blocs')} <code>&lt;&lt;&lt;&lt;&lt;&lt;&lt;</code> … <code>&gt;&gt;&gt;&gt;&gt;&gt;&gt;</code>
    {t('dans un autre éditeur, puis « Résolu à la main ».')}
  </p>
  <ul>
    {#each app.git?.conflicts ?? [] as path (path)}
      <li>
        <span class="path" title={path}>{path}</span>
        <button disabled={app.syncing} onclick={() => resolve(path, 'ours')}>{t('Garder ma version')}</button>
        <button disabled={app.syncing} onclick={() => resolve(path, 'theirs')}>{t('Garder la version distante')}</button>
        <button disabled={app.syncing} onclick={() => resolve(path, 'manual')}>{t('Résolu à la main')}</button>
      </li>
    {/each}
  </ul>
  <button class="abort" disabled={app.syncing} onclick={abortMerge}>{t('Annuler la fusion')}</button>
</main>

<style>
  .conflicts {
    flex: 1;
    min-height: 0;
    overflow: auto;
    max-width: 720px;
    width: 100%;
    margin: 0 auto;
    padding: 10vh 32px;
  }
  h1 { margin: 0 0 12px; font: 700 26px/1.2 var(--display); letter-spacing: -0.02em; }
  p { margin: 0 0 24px; color: var(--muted); font: 14.5px/1.6 var(--text); }
  code { font: 12.5px var(--mono); color: var(--fg); }
  ul { list-style: none; margin: 0 0 24px; padding: 0; border-top: 1px solid var(--line-soft); }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 10px 0;
    border-bottom: 1px solid var(--line-soft);
  }
  .path { flex: 1 1 200px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: 13px var(--mono); }
  button {
    padding: 6px 10px;
    border-radius: var(--r);
    border: 1px solid var(--line);
    color: var(--fg);
    font: 600 12.5px/1 var(--display);
    transition: background var(--t-fast) var(--ease);
  }
  button:hover:not(:disabled) { background: var(--orange-wash); }
  button:disabled { opacity: 0.5; }
  .abort { border-color: transparent; color: var(--muted); }
</style>
