<script lang="ts">
  import type { ClassValue } from 'svelte/elements'

  let { fraction, label, class: klass = 'w-20' }: { fraction: number; label: string; class?: ClassValue } = $props()

  const percent = $derived(Math.round(100 * fraction))
</script>

<span
  class={['block h-1 shrink-0 overflow-hidden rounded-full bg-line', klass]}
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={percent}
>
  <span class="block h-full rounded-full bg-accent transition-[width]" style:width="{100 * fraction}%"></span>
</span>
