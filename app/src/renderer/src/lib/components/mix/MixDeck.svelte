<script lang="ts">
  import { LoaderCircle, Music } from '@lucide/svelte'
  import { artworkUrl } from '$lib/api'
  import { bpmAt } from '$lib/beat'
  import { setPlayer } from '$lib/deck/context'
  import { formatBpm, formatClock, keyColor } from '$lib/format'
  import type { Player } from '$lib/player.svelte'
  import WaveformDetail from '../WaveformDetail.svelte'
  import WaveformOverview from '../WaveformOverview.svelte'

  /**
   * One side of the mixer: the track, where its playhead is, the scrolling waveform and an overview
   * split into the sections between cues. Set the playhead by clicking the overview or dragging the
   * scrolling waveform; the mixer's play starts it.
   */
  let {
    player,
    label,
    focused,
    onfocus
  }: {
    /** Fixed for the component's life: key the deck on it to swap. */
    player: Player
    label: string
    /** Double-clicked rows load here. */
    focused: boolean
    onfocus: () => void
  } = $props()

  // svelte-ignore state_referenced_locally
  setPlayer(player)

  const track = $derived(player.track)
  const bpm = $derived(bpmAt(player.position, player.displayGrid, track?.bpm ?? 0))
</script>

{#snippet placeholder()}
  <div class="grid h-full place-items-center px-4 text-center text-[12px] text-faint">
    {#if player.loading}
      <LoaderCircle class="size-4 animate-spin" />
    {:else if track}
      No analysis data for this track
    {:else}
      Double-click a track to load it on deck {label}
    {/if}
  </div>
{/snippet}

<section
  class={[
    'flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border bg-surface transition-colors',
    focused ? 'border-accent/60' : 'border-line-soft'
  ]}
  onpointerdowncapture={onfocus}
  aria-label="Deck {label}"
>
  <div class="relative h-[104px] border-b border-line-soft bg-[oklch(0.12_0.005_260)]">
    {#if player.waveform}
      <WaveformDetail />
    {:else}
      {@render placeholder()}
    {/if}
    <span
      class={[
        'pointer-events-none absolute top-2 left-2 grid size-6 place-items-center rounded text-[12px] font-bold',
        focused ? 'bg-accent text-bg' : 'bg-raised text-muted'
      ]}>{label}</span
    >
  </div>

  <div class="flex items-center gap-3 px-3 pt-3">
    <div class="size-11 shrink-0 overflow-hidden rounded-md bg-raised">
      {#if track?.hasArtwork}
        <img src={artworkUrl(track.id, 'm')} alt="" class="size-full object-cover" draggable="false" />
      {:else}
        <div class="grid size-full place-items-center text-faint"><Music class="size-4" /></div>
      {/if}
    </div>
    <div class="min-w-0 flex-1">
      {#if track}
        <div class="truncate text-[13.5px] font-semibold text-fg" title={track.title}>{track.title}</div>
        <div class="tabular mt-0.5 flex items-center gap-1.5 text-[11px]">
          <span class="truncate text-muted">{track.artist || 'Unknown artist'}</span>
          <span class="shrink-0 rounded bg-raised px-1.5 py-px font-semibold text-fg">{formatBpm(bpm)} BPM</span>
          {#if track.key}
            <span
              class="shrink-0 rounded bg-raised px-1.5 py-px font-semibold"
              style:color={keyColor(track.key) ?? 'inherit'}>{track.key}</span
            >
          {/if}
        </div>
      {:else}
        <div class="text-[13px] text-faint">No track loaded</div>
      {/if}
    </div>
    <div class="tabular shrink-0 text-right font-mono">
      <div class="text-[14px] font-medium text-fg">{formatClock(player.position * 1000)}</div>
      <div class="text-[11px] text-faint">−{formatClock(Math.max(0, player.duration - player.position) * 1000)}</div>
    </div>
  </div>

  <div class="px-3 pt-2 pb-3">
    <div class="relative h-16 overflow-hidden rounded bg-[oklch(0.12_0.005_260)]">
      <WaveformOverview sections />
    </div>
  </div>
</section>
