<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { ClassValue } from 'svelte/elements'
  import { LoaderCircle } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { getDeckInfo, getPlayer } from '$lib/deck/context'
  import { TrackDrop } from '$lib/deck/drop.svelte'
  import { deckName } from '$lib/deck/shortcut'
  import type { Track } from '$lib/types'
  import WaveformDetail from '../WaveformDetail.svelte'

  /**
   * What every deck is built in: a track dropped anywhere on it loads, and the scrolling waveform
   * runs across the top, or a note in its place while there is none. Each mode lays its own
   * overlays on the waveform and its own controls under it.
   */
  let {
    class: className,
    waveClass,
    empty,
    onload,
    onfocus,
    overlay,
    children
  }: {
    class?: ClassValue
    /** The waveform's height. */
    waveClass?: ClassValue
    /** Said in the waveform's place with no track loaded. */
    empty: string
    onload: (track: Track) => void
    /** A press anywhere on the deck, before whatever it lands on. */
    onfocus?: () => void
    /** Over the waveform: its toolbar, badges and buttons. */
    overlay?: Snippet
    children: Snippet
  } = $props()

  const { library } = getApp()
  const player = getPlayer()
  const { label } = getDeckInfo()
  // svelte-ignore state_referenced_locally
  const drop = new TrackDrop(library, (track) => onload(track))
</script>

<section
  class={['relative', className]}
  aria-label={label ? `Deck ${label}` : 'Deck'}
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
      <span class="rounded bg-bg/80 px-2 py-1 text-[12px] font-semibold text-accent">Load on {deckName(label)}</span>
    </div>
  {/if}
  <div class={['relative border-b border-line-soft bg-wave', waveClass]}>
    {#if player.waveform}
      <WaveformDetail />
    {:else}
      <div class="grid h-full place-items-center px-4 text-center text-[12px] text-faint">
        {#if player.loading}
          <LoaderCircle class="size-4 animate-spin" />
        {:else if player.track}
          No analysis data for this track
        {:else}
          {empty}
        {/if}
      </div>
    {/if}
    {@render overlay?.()}
  </div>
  {@render children()}
</section>
