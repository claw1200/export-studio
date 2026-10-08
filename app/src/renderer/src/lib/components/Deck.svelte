<script lang="ts">
  import { Play, Pause, Minus, Plus, Music, LoaderCircle, Pencil, X, Link, Unlink } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { artworkUrl } from '$lib/api'
  import {
    formatBpm,
    formatClock,
    HOT_CUE_KINDS,
    HOT_CUE_LETTERS,
    HOT_CUE_PICKER,
    hotCueLetter,
    hotCuePaletteColor,
    keyColor
  } from '$lib/format'
  import { bpmAt } from '$lib/beat'
  import { chordLabel } from '$lib/keymap'
  import type { Cue } from '$lib/types'
  import { cueColor, type WaveformMode } from '$lib/waveform'
  import WaveformDetail from './WaveformDetail.svelte'
  import WaveformOverview from './WaveformOverview.svelte'
  import BpmField from './BpmField.svelte'

  const { library, player, keybinds } = getApp()

  const MODES: { id: WaveformMode; label: string }[] = [
    { id: 'rgb', label: 'RGB' },
    { id: '3band', label: '3-Band' }
  ]

  const track = $derived(player.track)
  const hotCues = $derived.by(() => {
    const bySlot = new Map(
      (player.detail?.cues ?? []).filter((c) => c.kind === 'hot').map((c) => [hotCueLetter(c.slot), c])
    )
    return [...HOT_CUE_LETTERS].map((letter, i) => ({
      letter,
      kind: HOT_CUE_KINDS[i],
      cue: bySlot.get(letter) ?? null
    }))
  })
  const editable = $derived(library.writable && !!player.detail && !player.gridEditing)
  const gridEditable = $derived(library.writable && player.canEditGrid)
  let colorPickerCueId = $state<string | null>(null)

  $effect(() => {
    if (player.gridEditing && !library.writable) player.cancelGridEdit()
  })

  async function saveGrid(): Promise<void> {
    await player.saveGrid()
    const saved = player.track
    if (saved && !player.gridEditing) library.setTrackBpm(saved.id, saved.bpm)
  }

  function toggleGridEdit(): void {
    if (player.gridEditing) void saveGrid()
    else if (gridEditable) {
      colorPickerCueId = null
      player.beginGridEdit()
    }
  }

  $effect(() => keybinds.register({ 'deck.gridEdit': toggleGridEdit }))

  const gridShortcut = $derived(keybinds.bindings['deck.gridEdit'][0])
  const gridTitle = $derived(
    (!library.writable
      ? 'Unlock editing to change the beatgrid'
      : player.canEditGrid
        ? 'Edit the beatgrid'
        : 'This track has no beatgrid') + (gridShortcut ? ` (${chordLabel(gridShortcut)})` : '')
  )

  function pickColor(cue: Cue, colorIndex: number | null): void {
    colorPickerCueId = null
    void player.setCueColor(cue, colorIndex)
  }
  const memoryCount = $derived((player.detail?.cues ?? []).filter((c) => c.kind === 'memory').length)
  const cueLit = $derived(player.cueHeld || (!player.playing && !!track && player.atCue))
  const loopButton = (lit: boolean): string[] => [
    'flex h-7 items-center rounded-md border px-2 text-[10.5px] font-bold tracking-wide transition disabled:opacity-30',
    lit
      ? 'border-transparent bg-cue text-bg shadow-[0_0_10px] shadow-cue/50'
      : 'border-line-soft text-faint hover:bg-hover hover:text-muted'
  ]
  const placementLabel = $derived(player.cuePlacement === 'cuePoint' ? 'cue point' : 'playhead')
  const cueShortcut = $derived(keybinds.bindings['deck.cue'][0])
  const cueTitle = $derived(
    (player.playing
      ? 'Return to the cue point and stop'
      : player.atCue
        ? 'Hold to play from the cue point'
        : `Set the cue point here${player.quantise ? ', on the nearest beat' : ''}`) +
      (cueShortcut ? ` (${chordLabel(cueShortcut)})` : '')
  )
  const currentBpm = $derived(bpmAt(player.position, player.displayGrid, track?.bpm ?? 0))
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape' || e.defaultPrevented) return
    if (colorPickerCueId) colorPickerCueId = null
    else if (player.gridEditing && !player.editing) player.cancelGridEdit()
  }}
/>

{#snippet gridButton(label: string, title: string, onclick: (e: MouseEvent) => void)}
  <button
    class="tabular h-6 min-w-7 rounded border border-line-soft px-1.5 font-mono text-[11px] font-semibold text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
    disabled={player.editing}
    aria-label={title}
    {title}
    {onclick}
  >
    {label}
  </button>
{/snippet}

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
    {#if player.gridEditing}
      <div class="flex h-14 w-[328px] shrink-0 flex-col justify-between" role="toolbar" aria-label="Beatgrid editor">
        <div class="flex items-center gap-1">
          <span class="mr-auto flex items-baseline gap-1">
            <BpmField bpm={currentBpm} />
            <span class="text-[11px] font-medium text-faint">BPM</span>
          </span>
          {@render gridButton('←', 'Shift the beatgrid left by 1 ms (Shift: 10 ms)', (e) =>
            player.nudgeGrid(e.shiftKey ? -10 : -1)
          )}
          {@render gridButton('→', 'Shift the beatgrid right by 1 ms (Shift: 10 ms)', (e) =>
            player.nudgeGrid(e.shiftKey ? 10 : 1)
          )}
          {@render gridButton('/2', 'Halve the BPM', () => player.halveGrid())}
          {@render gridButton('×2', 'Double the BPM', () => player.doubleGrid())}
        </div>
        <div class="flex items-center gap-1">
          {@render gridButton('Set', 'Move the nearest beat to the playhead and make it the downbeat', () =>
            player.setDownbeatHere()
          )}
          <button
            class={[
              'grid size-6 place-items-center rounded border transition',
              player.gridLink
                ? 'border-transparent bg-accent-soft text-accent'
                : 'border-line-soft text-faint hover:bg-hover hover:text-muted'
            ]}
            onclick={() => player.setGridLink(!player.gridLink)}
            aria-pressed={player.gridLink}
            aria-label="Move cues with the grid"
            title={player.gridLink
              ? 'Hot cues and memory cues move with the grid'
              : 'Cues stay where they are when the grid moves'}
          >
            {#if player.gridLink}
              <Link class="size-3.5" />
            {:else}
              <Unlink class="size-3.5" />
            {/if}
          </button>
          <button
            class="ml-auto h-6 rounded px-2 text-[11px] font-semibold text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
            disabled={player.editing}
            onclick={() => player.cancelGridEdit()}
            title="Discard grid changes (Esc)"
          >
            Cancel
          </button>
          <button
            class="flex h-6 items-center gap-1 rounded bg-accent px-2.5 text-[11px] font-semibold text-bg hover:brightness-110 disabled:opacity-40"
            disabled={player.editing}
            onclick={saveGrid}
            title={player.gridDirty ? 'Save the beatgrid to rekordbox' : 'No changes to save'}
          >
            {#if player.editing}<LoaderCircle class="size-3 animate-spin" />{/if}
            Save
          </button>
        </div>
      </div>
    {:else}
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
            <button
              class="ml-auto flex items-center gap-1 rounded border border-line-soft px-1.5 py-px font-semibold text-faint transition hover:bg-hover hover:text-muted disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-faint"
              disabled={!gridEditable}
              onclick={toggleGridEdit}
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
    {/if}

    <div class="h-12 min-w-0 flex-1">
      <WaveformOverview />
    </div>

    <div class="tabular w-24 shrink-0 text-right font-mono">
      <div class="text-[15px] font-medium text-fg">{formatClock(player.position * 1000)}</div>
      <div class="text-[11px] text-faint">−{formatClock((player.duration - player.position) * 1000)}</div>
    </div>

    <div class="flex shrink-0 flex-col items-center gap-1.5">
      <button
        class="grid size-11 place-items-center rounded-full bg-accent text-bg transition hover:brightness-110 disabled:opacity-30"
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
      <button
        class={[
          'h-6 w-11 touch-none rounded-full border text-[10.5px] font-bold tracking-wide transition select-none disabled:opacity-30',
          cueLit
            ? 'border-transparent bg-cue text-bg shadow-[0_0_10px] shadow-cue/50'
            : 'border-cue/60 text-cue hover:bg-cue/15'
        ]}
        disabled={!track || !!player.audioError}
        onpointerdown={(e) => {
          if (e.button !== 0) return
          e.currentTarget.setPointerCapture(e.pointerId)
          player.cueDown()
        }}
        onpointerup={() => player.cueUp()}
        onpointercancel={() => player.cueUp()}
        onlostpointercapture={() => player.cueUp()}
        aria-label="Cue"
        title={cueTitle}
      >
        CUE
      </button>
    </div>
  </div>

  <div class="flex items-center gap-1.5 px-4 pb-3">
    {#each hotCues as { letter, kind, cue } (letter)}
      {#if cue}
        <div class="group relative">
          <button
            class="flex h-7 w-[72px] touch-none items-center gap-1.5 rounded-md px-2 text-[11px] font-semibold text-bg transition select-none hover:brightness-110"
            style:background={cueColor(cue)}
            title={cue.comment || undefined}
            onpointerdown={(e) => {
              if (e.button !== 0) return
              e.currentTarget.setPointerCapture(e.pointerId)
              player.hotCueDown(kind)
            }}
            onpointerup={() => player.hotCueUp(kind)}
            onpointercancel={() => player.hotCueUp(kind)}
            onlostpointercapture={() => player.hotCueUp(kind)}
          >
            {letter}
            <span
              class={[
                'tabular truncate font-mono text-[10px] font-medium opacity-75 transition-opacity',
                editable && ['group-hover:opacity-0', colorPickerCueId === cue.id && 'opacity-0']
              ]}
            >
              {formatClock(cue.start).slice(0, 5)}
            </span>
          </button>
          {#if editable}
            <div
              class={[
                'absolute top-1/2 right-1 flex -translate-y-1/2 opacity-0 group-hover:opacity-100 focus-within:opacity-100',
                colorPickerCueId === cue.id && 'opacity-100'
              ]}
            >
              <button
                class="grid size-5 place-items-center rounded text-bg hover:bg-black/25 disabled:hidden"
                disabled={player.editing}
                onclick={() => (colorPickerCueId = colorPickerCueId === cue.id ? null : cue.id)}
                aria-label="Change colour of hot cue {letter}"
                aria-expanded={colorPickerCueId === cue.id}
                aria-haspopup="dialog"
                title="Change colour"
              >
                <Pencil class="size-3" />
              </button>
              <button
                class="grid size-5 place-items-center rounded text-bg hover:bg-black/25 disabled:hidden"
                disabled={player.editing}
                onclick={() => player.deleteCue(cue)}
                aria-label="Delete hot cue {letter}"
                title="Delete hot cue {letter}"
              >
                <X class="size-3" />
              </button>
            </div>
          {/if}
          {#if editable && colorPickerCueId === cue.id}
            <button
              class="fixed inset-0 z-40 cursor-default"
              aria-label="Close colour picker"
              onclick={() => (colorPickerCueId = null)}
            ></button>
            <div
              class="absolute top-full left-0 z-50 mt-1.5 w-max rounded-lg border border-line bg-surface p-2 shadow-xl shadow-black/40"
              role="dialog"
              aria-label="Hot cue {letter} colour"
            >
              <div class="grid grid-cols-4 gap-1.5">
                {#each HOT_CUE_PICKER as index (index)}
                  <button
                    class={[
                      'size-6 rounded transition hover:scale-110',
                      cue.colorIndex === index && 'ring-2 ring-fg ring-offset-1 ring-offset-surface'
                    ]}
                    style:background={hotCuePaletteColor(index)}
                    onclick={() => pickColor(cue, index)}
                    aria-label="Colour {index}"
                    aria-pressed={cue.colorIndex === index}
                  ></button>
                {/each}
              </div>
              <button
                class="mt-2 w-full rounded-md px-2 py-1 text-left text-[11px] text-muted hover:bg-hover hover:text-fg disabled:opacity-50"
                disabled={!cue.colorIndex}
                onclick={() => pickColor(cue, null)}
              >
                No colour
              </button>
            </div>
          {/if}
        </div>
      {:else if editable}
        <button
          class="group flex h-7 w-[72px] items-center gap-1.5 rounded-md border border-dashed border-line px-2 text-[11px] font-semibold text-faint transition hover:border-accent/60 hover:bg-accent-soft hover:text-accent disabled:pointer-events-none disabled:opacity-50"
          disabled={player.editing}
          onclick={() => player.setHotCue(kind)}
          aria-label="Set hot cue {letter} at the {placementLabel}"
          title="Set hot cue {letter} at the {placementLabel}{player.quantise ? ', on the nearest beat' : ''}"
        >
          {letter}
          <Plus class="ml-auto size-3 opacity-0 group-hover:opacity-100" />
        </button>
      {:else}
        <div
          class="flex h-7 w-[72px] items-center rounded-md border border-line-soft px-2 text-[11px] font-semibold text-faint/60"
        >
          {letter}
        </div>
      {/if}
    {/each}

    <span class="ml-3 text-[11px] text-faint">{memoryCount} memory cues</span>

    <div class="ml-auto flex items-center gap-3">
      <div class="flex items-center gap-1" role="group" aria-label="Loop">
        <button
          class={loopButton(false)}
          disabled={!track || !!player.audioError}
          onclick={() => player.setLoopIn()}
          title="Loop in"
        >
          IN
        </button>
        <button
          class={loopButton(false)}
          disabled={!track || !!player.audioError}
          onclick={() => player.setLoopOut()}
          title="Loop out"
        >
          OUT
        </button>
        <button
          class={loopButton(!!player.loop?.active)}
          disabled={!player.loop}
          onclick={() => player.reloopExit()}
          aria-pressed={!!player.loop?.active}
          title={player.loop?.active ? 'Exit loop' : 'Reloop'}
        >
          {player.loop?.active ? 'EXIT' : 'RELOOP'}
        </button>
        <button
          class={loopButton(false)}
          disabled={!track || !!player.audioError}
          onclick={() => player.beatLoop(4)}
          title="4-beat loop"
        >
          4 BEAT
        </button>
        <button
          class={loopButton(false)}
          disabled={!player.loop}
          onclick={() => player.resizeLoop(0.5)}
          aria-label="Halve loop"
          title="Halve loop"
        >
          ½
        </button>
        <button
          class={loopButton(false)}
          disabled={!player.loop}
          onclick={() => player.resizeLoop(2)}
          aria-label="Double loop"
          title="Double loop"
        >
          ×2
        </button>
      </div>
      <button
        class={[
          'flex h-7 items-center rounded-md border px-2 text-[11px] font-semibold transition',
          player.quantise
            ? 'border-transparent bg-accent-soft text-accent'
            : 'border-line-soft text-faint hover:bg-hover hover:text-muted'
        ]}
        onclick={() => player.setQuantise(!player.quantise)}
        aria-pressed={player.quantise}
        title={player.quantise ? 'Quantise on. Scrubbing and new hot cues snap to the beat.' : 'Quantise off'}
      >
        Quantise
      </button>
    </div>
  </div>
</section>
