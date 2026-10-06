<script lang="ts">
  import { Play, Pause, Minus, Plus, Music, AlertTriangle, LoaderCircle } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { artworkUrl } from '$lib/api'
  import { formatBpm, formatClock, hotCueLetter, keyColor } from '$lib/format'
  import { cueColor, type WaveformMode } from '$lib/waveform'
  import WaveformDetail from './WaveformDetail.svelte'
  import WaveformOverview from './WaveformOverview.svelte'

  const { player } = getApp()

  const MODES: { id: WaveformMode; label: string }[] = [
    { id: 'rgb', label: 'RGB' },
    { id: '3band', label: '3-Band' }
  ]

  const track = $derived(player.track)
  const hotCues = $derived.by(() => {
    const bySlot = new Map(
      (player.detail?.cues ?? []).filter((c) => c.kind === 'hot').map((c) => [hotCueLetter(c.slot), c])
    )
    return [...'ABCDEFGH'].map((letter) => ({ letter, cue: bySlot.get(letter) ?? null }))
  })
  const memoryCount = $derived((player.detail?.cues ?? []).filter((c) => c.kind === 'memory').length)

  const currentBpm = $derived.by(() => {
    const grid = player.detail?.beatgrid
    if (!grid || !grid.times.length) return track?.bpm ?? 0
    const ms = player.position * 1000
    let lo = 0
    let hi = grid.times.length - 1
    while (lo < hi) {
      const m = (lo + hi + 1) >> 1
      if (grid.times[m] <= ms) lo = m
      else hi = m - 1
    }
    return grid.bpms[lo]
  })
</script>

<section class="shrink-0 border-b border-line-soft bg-surface">
  <div class="relative h-[136px] border-b border-line-soft bg-[oklch(0.12_0.005_260)]">
    {#if player.waveform}
      <WaveformDetail />
    {:else}
      <div class="grid h-full place-items-center text-[12px] text-faint">
        {#if player.loading}
          <LoaderCircle class="size-4 animate-spin" />
        {:else if track}
          No analysis data for this track
        {:else}
          Select a track to view its waveform
        {/if}
      </div>
    {/if}

    <div class="absolute top-2 right-2 flex items-center gap-1.5">
      <div class="flex rounded-md border border-line bg-bg/80 p-0.5 text-[10.5px] font-semibold backdrop-blur">
        {#each MODES as m (m.id)}
          <button
            class={['rounded px-2 py-0.5', player.mode === m.id ? 'bg-raised text-fg' : 'text-faint hover:text-muted']}
            onclick={() => player.setMode(m.id)}
          >
            {m.label}
          </button>
        {/each}
      </div>
      <div class="flex items-center rounded-md border border-line bg-bg/80 backdrop-blur">
        <button
          class="grid size-6 place-items-center text-faint hover:text-fg"
          onclick={() => player.setZoom(player.zoom / 1.5)}
          aria-label="Zoom out"
        >
          <Minus class="size-3" />
        </button>
        <button
          class="grid size-6 place-items-center text-faint hover:text-fg"
          onclick={() => player.setZoom(player.zoom * 1.5)}
          aria-label="Zoom in"
        >
          <Plus class="size-3" />
        </button>
      </div>
    </div>
  </div>

  <div class="flex items-center gap-4 px-4 py-3">
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
          <span class="rounded bg-raised px-1.5 py-0.5 font-semibold text-fg">{formatBpm(currentBpm)} BPM</span>
          {#if track.key}
            <span class="rounded bg-raised px-1.5 py-0.5 font-semibold" style:color={keyColor(track.key) ?? 'inherit'}
              >{track.key}</span
            >
          {/if}
          <span class="rounded bg-raised px-1.5 py-0.5 text-muted">{track.fileType}</span>
        </div>
      {:else}
        <div class="text-[13px] text-faint">No track loaded</div>
      {/if}
    </div>

    <div class="h-12 min-w-0 flex-1">
      <WaveformOverview />
    </div>

    <div class="tabular w-24 shrink-0 text-right font-mono">
      <div class="text-[15px] font-medium text-fg">{formatClock(player.position * 1000)}</div>
      <div class="text-[11px] text-faint">−{formatClock((player.duration - player.position) * 1000)}</div>
    </div>

    <button
      class="grid size-11 shrink-0 place-items-center rounded-full bg-accent text-bg transition hover:brightness-110 disabled:opacity-30"
      disabled={!track || !!player.audioError}
      onclick={() => player.toggle()}
      aria-label={player.playing ? 'Pause' : 'Play'}
    >
      {#if player.playing}
        <Pause class="size-5 fill-current" />
      {:else}
        <Play class="ml-0.5 size-5 fill-current" />
      {/if}
    </button>
  </div>

  <div class="flex items-center gap-1.5 px-4 pb-3">
    {#each hotCues as { letter, cue } (letter)}
      <button
        class={[
          'flex h-7 w-[72px] items-center gap-1.5 rounded-md border px-2 text-[11px] font-semibold transition',
          cue ? 'border-transparent text-bg hover:brightness-110' : 'border-line-soft text-faint/60'
        ]}
        style:background={cue ? cueColor(cue) : undefined}
        disabled={!cue}
        title={cue?.comment || undefined}
        onclick={() => cue && player.seek(cue.start / 1000)}
      >
        {letter}
        {#if cue}
          <span class="tabular truncate font-mono text-[10px] font-medium opacity-75">
            {formatClock(cue.start).slice(0, 5)}
          </span>
        {/if}
      </button>
    {/each}

    <span class="ml-3 text-[11px] text-faint">{memoryCount} memory cues</span>

    {#if player.audioError}
      <span class="ml-auto flex items-center gap-1.5 text-[11px] text-warn">
        <AlertTriangle class="size-3.5" />
        {player.audioError}
      </span>
    {/if}
  </div>
</section>
