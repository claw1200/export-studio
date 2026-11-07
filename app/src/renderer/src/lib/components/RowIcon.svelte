<!--
@component
Lucide glyphs as static SVG, for places that mount many at once (track table rows, rating stars).
The `@lucide/svelte` components build each icon through two components, spread props and dynamic
elements; a row of the browser can hold several, so scrolling mounted dozens per frame. The paths
are copied from Lucide (ISC licence) and drawn with Lucide's default attributes.
-->
<script lang="ts">
  import type { ClassValue } from 'svelte/elements'

  let {
    name,
    class: klass = '',
    strokeWidth = 2
  }: { name: 'star' | 'music' | 'audio-lines' | 'x'; class?: ClassValue; strokeWidth?: number } = $props()
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  class={['lucide-icon lucide', klass]}
  aria-hidden="true"
>
  {#if name === 'star'}
    <path
      d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z"
    />
  {:else if name === 'music'}
    <path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" />
  {:else if name === 'audio-lines'}
    <path d="M2 10v3" /><path d="M6 6v11" /><path d="M10 3v18" /><path d="M14 8v7" /><path d="M18 5v13" /><path
      d="M22 10v3"
    />
  {:else}
    <path d="M18 6 6 18" /><path d="m6 6 12 12" />
  {/if}
</svg>
