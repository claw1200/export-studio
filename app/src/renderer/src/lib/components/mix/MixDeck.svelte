<script lang="ts">
  import { LoaderCircle, Music, Repeat } from '@lucide/svelte'
  import { artworkUrl } from '$lib/api'
  import { bpmAt } from '$lib/beat'
  import { setPlayer } from '$lib/deck/context'
  import { cueColor, formatBpm, formatClock, hotCueLetter, keyColor } from '$lib/format'
  import type { Mixer } from '$lib/mixer.svelte'
  import type { Player } from '$lib/player.svelte'
  import type { Cue } from '$lib/types'
  import WaveformDetail from '../WaveformDetail.svelte'
  import WaveformOverview from '../WaveformOverview.svelte'

  /**
   * One side of the mixer: where the deck is in its track, and its cues to start it from. Set the
   * playhead with a cue, the overview or by dragging the waveform; the mixer's play starts it.
   */
  let {
    player,
    mixer,
    label,
    focused,
    onfocus
  }: {
    /** Fixed for the component's life: key the deck on it to swap. */
    player: Player
    mixer: Mixer
    label: string
    /** Double-clicked rows load here. */
    focused: boolean
    onfocus: () => void
  } = $props()

  // svelte-ignore state_referenced_locally
  setPlayer(player)

  const track = $derived(player.track)
  const bpm = $derived(bpmAt(player.position, player.displayGrid, track?.bpm ?? 0))
  const cues = $derived([...player.displayCues].sort((a, b) => a.start - b.start || a.slot - b.slot))
  /** Within a millisecond counts as sitting on a cue. */
  const atCue = (cue: Cue): boolean => Math.abs(player.position - cue.start / 1000) < 0.001

  function cueLabel(cue: Cue): string {
    return cue.kind === 'hot' ? hotCueLetter(cue.slot) : 'Memory'
  }

  /** Puts the playhead on the cue: a jump while playing, a new start point while stopped. */
  function startFrom(cue: Cue): void {
    const seconds = cue.start / 1000
    if (player.playing) player.seek(seconds)
    else player.stopAt(seconds)
    mixer.placed(player, seconds)
  }
</script>

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
      <div class="grid h-full place-items-center px-4 text-center text-[12px] text-faint">
        {#if player.loading}
          <LoaderCircle class="size-4 animate-spin" />
        {:else if track}
          No analysis data for this track
        {:else}
          Double-click a track to load it on deck {label}
        {/if}
      </div>
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

  <div class="h-10 px-3 pt-2">
    <WaveformOverview />
  </div>

  <div class="flex min-h-[44px] flex-wrap content-start gap-1 px-3 pt-2 pb-3">
    {#each cues as cue (cue.id)}
      <button
        class={[
          'tabular flex h-6 items-center gap-1.5 rounded border px-1.5 text-[11px] transition-colors',
          atCue(cue) ? 'border-fg/70 bg-hover text-fg' : 'border-line-soft text-muted hover:bg-hover hover:text-fg'
        ]}
        title="Start deck {label} from here{cue.comment ? `: ${cue.comment}` : ''}"
        disabled={!!player.audioError}
        onclick={() => startFrom(cue)}
      >
        {#if cue.kind === 'hot'}
          <span
            class="grid size-4 place-items-center rounded-sm text-[9.5px] font-bold text-bg"
            style:background={cueColor(cue)}>{cueLabel(cue)}</span
          >
        {:else}
          <span class="grid size-4 place-items-center" aria-label={cueLabel(cue)}>
            <span class="size-1.5 rotate-45 rounded-[1px]" style:background={cueColor(cue)}></span>
          </span>
        {/if}
        <span class="font-mono">{formatClock(cue.start)}</span>
        {#if cue.end != null}
          <Repeat class="size-2.5 text-faint" />
        {/if}
      </button>
    {:else}
      {#if track && !player.loading}
        <span class="self-center text-[11.5px] text-faint"
          >No cues. Click the overview or drag the waveform to set a start.</span
        >
      {/if}
    {/each}
  </div>
</section>
