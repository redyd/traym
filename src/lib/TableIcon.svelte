<script lang="ts" module>
  export type TableOp = 'row-above' | 'row-below' | 'col-left' | 'col-right' | 'row-del' | 'col-del'
</script>

<script lang="ts">
  /** Lucide-style table icons: a "+" on the side a row/column is inserted, or the doomed row/column shaded with a "×". */
  let { kind, size = 15, strokeWidth = 1.6 }: { kind: TableOp; size?: number; strokeWidth?: number } = $props()
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={strokeWidth} stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if kind === 'row-above'}
    <rect x="3" y="11" width="18" height="10" rx="2" /><path d="M3 16h18M12 11v10M12 2.5v6M9 5.5h6" />
  {:else if kind === 'row-below'}
    <rect x="3" y="3" width="18" height="10" rx="2" /><path d="M3 8h18M12 3v10M12 15.5v6M9 18.5h6" />
  {:else if kind === 'col-left'}
    <rect x="11" y="3" width="10" height="18" rx="2" /><path d="M16 3v18M11 12h10M2.5 12h6M5.5 9v6" />
  {:else if kind === 'col-right'}
    <rect x="3" y="3" width="10" height="18" rx="2" /><path d="M8 3v18M3 12h10M15.5 12h6M18.5 9v6" />
  {:else if kind === 'row-del'}
    <rect x="3" y="9" width="11" height="6" fill="currentColor" fill-opacity="0.3" stroke="none" />
    <rect x="3" y="3" width="11" height="18" rx="2" /><path d="M3 9h11M3 15h11M17 9.5l5 5M22 9.5l-5 5" />
  {:else if kind === 'col-del'}
    <rect x="9" y="3" width="6" height="11" fill="currentColor" fill-opacity="0.3" stroke="none" />
    <rect x="3" y="3" width="18" height="11" rx="2" /><path d="M9 3v11M15 3v11M9.5 17l5 5M14.5 17l-5 5" />
  {/if}
</svg>
