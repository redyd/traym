<script lang="ts">
  import { Tooltip, type TooltipTriggerProps } from 'bits-ui'
  import type { Snippet } from 'svelte'

  /** A button with an accessible name and a tooltip that is always kept inside the window. */
  let { label, children, ...rest }: { label: string; children: Snippet } & Omit<TooltipTriggerProps, 'children'> = $props()
</script>

<Tooltip.Root>
  <Tooltip.Trigger aria-label={label} {...rest}>{@render children()}</Tooltip.Trigger>
  <Tooltip.Portal>
    <Tooltip.Content class="tip" side="bottom" sideOffset={6} collisionPadding={8}>{label}</Tooltip.Content>
  </Tooltip.Portal>
</Tooltip.Root>
