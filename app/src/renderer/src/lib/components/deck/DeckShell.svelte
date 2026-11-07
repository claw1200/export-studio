<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { ClassValue } from 'svelte/elements'
  import { LoaderCircle } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { deckName, getDeckInfo, getPlayer } from '$lib/deck/context'
  import { TrackDrop } from '$lib/deck/drop.svelte'
  import type { Track } from '$lib/types'
  import WaveformDetail from '../WaveformDetail.svelte'

  let {
    class: className,
    waveformClass,
    emptyMessage,
    onload,
    onfocus,
    waveformOverlay,
    children
  }: {
    class?: ClassValue
    waveformClass?: ClassValue
    emptyMessage: string
    onload: (track: Track) => void
    onfocus?: () => void
    waveformOverlay?: Snippet
    children: Snippet
  } = $props()

  const { library } = getApp()
  const player = getPlayer()
  const deck = getDeckInfo()
  // svelte-ignore state_referenced_locally
  const drop = new TrackDrop(library, onload)
</script>

<section
  class={['relative', className]}
  aria-label={deck.label ? `Deck ${deck.label}` : 'Deck'}
  onpointerdowncapture={onfocus}
  ondragenter={drop.ondragenter}
  ondragover={drop.ondragover}
  ondragleave={drop.ondragleave}
  ondrop={drop.ondrop}
>
  {#if drop.over}
    <div
      class="pointer-events-none absolute inset-0 z-30 grid place-items-center rounded-[inherit] border-2 border-accent bg-accent-soft"
    >
      <span class="rounded bg-bg/80 px-2 py-1 text-[12px] font-semibold text-accent">Load on {deckName(deck)}</span>
    </div>
  {/if}
  <div class={['relative border-b border-line-soft bg-wave', waveformClass]}>
    {#if player.waveform}
      <WaveformDetail />
    {:else}
      <div class="grid h-full place-items-center px-4 text-center text-[12px] text-faint">
        {#if player.loading}
          <LoaderCircle class="size-4 animate-spin" />
        {:else if player.track}
          No analysis data for this track
        {:else}
          {emptyMessage}
        {/if}
      </div>
    {/if}
    {@render waveformOverlay?.()}
  </div>
  {@render children()}
</section>
