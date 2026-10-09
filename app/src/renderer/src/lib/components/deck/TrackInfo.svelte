<script lang="ts">
  import { Music, Pencil } from '@lucide/svelte'
  import { artworkUrl } from '$lib/api'
  import { getApp } from '$lib/context'
  import { formatBpm, keyColor, pitchSemitones, shiftKey } from '$lib/format'

  const {
    bpm,
    gridEditable,
    gridTitle,
    ongridedit
  }: { bpm: number; gridEditable: boolean; gridTitle: string; ongridedit: () => void } = $props()

  const { player } = getApp()
  const track = $derived(player.track)
  const rate = $derived(player.tempo.rate)
  /** Master tempo holds the pitch, so the key stays. Otherwise it moves with the fader. */
  const semitones = $derived(player.tempo.masterTempo ? 0 : pitchSemitones(rate))
  const key = $derived(track?.key ? shiftKey(track.key, semitones) : '')
</script>

<div class="size-14 shrink-0 overflow-hidden rounded-md bg-raised shadow-lg shadow-black/40">
  {#if track?.hasArtwork}
    <img src={artworkUrl(track.id, 'm')} alt="" class="size-full object-cover" draggable="false" />
  {:else}
    <div class="grid size-full place-items-center text-faint"><Music class="size-5" /></div>
  {/if}
</div>

<div class="w-64 min-w-0 shrink-0">
  {#if track}
    <div class="truncate text-[14px] font-semibold text-fg" title={track.title}>{track.title}</div>
    <div class="truncate text-[12.5px] text-muted">{track.artist || 'Unknown artist'}</div>
    <div class="tabular mt-1.5 flex items-center gap-1.5 text-[11px]">
      <span
        class={['rounded bg-raised px-1.5 py-0.5 font-semibold', rate === 1 ? 'text-fg' : 'text-accent']}
        title={rate === 1 ? undefined : `${formatBpm(bpm)} BPM at the track's own tempo`}
        >{formatBpm(bpm * rate)} BPM</span
      >
      {#if key}
        <span
          class="rounded bg-raised px-1.5 py-0.5 font-semibold"
          style:color={keyColor(key) ?? 'inherit'}
          title={key === track.key ? undefined : `${track.key} at the track's own pitch`}
          >{key}</span
        >
      {/if}
      <span class="rounded bg-raised px-1.5 py-0.5 text-muted">{track.fileType}</span>
      <button
        class="ml-auto flex items-center gap-1 rounded border border-line-soft px-1.5 py-px font-semibold text-faint transition hover:bg-hover hover:text-muted disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-faint"
        disabled={!gridEditable}
        onclick={ongridedit}
        aria-label="Edit beatgrid"
        title={gridTitle}
      >
        <Pencil class="size-2.5" />
        Grid
      </button>
    </div>
  {:else}
    <div class="text-[13px] text-faint">No track loaded</div>
  {/if}
</div>
