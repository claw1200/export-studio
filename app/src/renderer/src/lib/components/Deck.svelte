<script lang="ts">
  import { LoaderCircle } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { bpmAt } from '$lib/beat'
  import { chordLabel } from '$lib/keymap'
  import WaveformDetail from './WaveformDetail.svelte'
  import WaveformOverview from './WaveformOverview.svelte'
  import TempoFader from './TempoFader.svelte'
  import WaveformToolbar from './deck/WaveformToolbar.svelte'
  import GridEditorBar from './deck/GridEditorBar.svelte'
  import TrackInfo from './deck/TrackInfo.svelte'
  import Transport from './deck/Transport.svelte'
  import HotCueBar from './deck/HotCueBar.svelte'
  import LoopControls from './deck/LoopControls.svelte'

  const { library, player, keybinds } = getApp()

  const track = $derived(player.track)
  const gridEditable = $derived(library.writable && player.grid.canEdit)
  const currentBpm = $derived(bpmAt(player.position, player.displayGrid, track?.bpm ?? 0))

  $effect(() => {
    if (player.grid.editing && !library.writable) player.grid.cancel()
  })

  async function saveGrid(): Promise<void> {
    await player.grid.save()
    const saved = player.track
    if (saved && !player.grid.editing) library.setTrackBpm(saved.id, saved.bpm)
  }

  function toggleGridEdit(): void {
    if (player.grid.editing) void saveGrid()
    else if (gridEditable) player.grid.begin()
  }

  $effect(() => keybinds.register({ 'deck.gridEdit': toggleGridEdit }))

  const gridShortcut = $derived(keybinds.bindings['deck.gridEdit'][0])
  const gridTitle = $derived(
    (!library.writable
      ? 'Unlock editing to change the beatgrid'
      : player.grid.canEdit
        ? 'Edit the beatgrid'
        : 'This track has no beatgrid') + (gridShortcut ? ` (${chordLabel(gridShortcut)})` : '')
  )
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape' || e.defaultPrevented) return
    if (player.grid.editing && !player.editing) player.grid.cancel()
  }}
/>

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
    <WaveformToolbar />
  </div>

  <div class="flex">
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-4 px-4 py-3">
        {#if player.grid.editing}
          <GridEditorBar bpm={currentBpm} onsave={saveGrid} />
        {:else}
          <TrackInfo bpm={currentBpm} {gridEditable} {gridTitle} ongridedit={toggleGridEdit} />
        {/if}

        <div class="h-12 min-w-0 flex-1">
          <WaveformOverview />
        </div>

        <Transport />
      </div>

      <div class="flex items-center gap-1.5 px-4 pb-3">
        <HotCueBar />
        <LoopControls />
      </div>
    </div>
    <TempoFader />
  </div>
</section>
