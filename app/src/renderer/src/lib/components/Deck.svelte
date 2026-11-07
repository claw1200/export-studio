<script lang="ts">
  import { LoaderCircle } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { setPlayer } from '$lib/deck/context'
  import { TrackDrop } from '$lib/deck/drop.svelte'
  import { deckKeys } from '$lib/deck/keys'
  import type { Player } from '$lib/player.svelte'
  import { bpmAt } from '$lib/beat'
  import { chordLabel } from '$lib/keymap'
  import WaveformDetail from './WaveformDetail.svelte'
  import WaveformOverview from './WaveformOverview.svelte'
  import TempoFader from './TempoFader.svelte'
  import WaveformToolbar from './deck/WaveformToolbar.svelte'
  import GridEditorBar from './deck/GridEditorBar.svelte'
  import TempoLane from './deck/TempoLane.svelte'
  import TrackInfo from './deck/TrackInfo.svelte'
  import Transport from './deck/Transport.svelte'
  import HotCueBar from './deck/HotCueBar.svelte'
  import LoopControls from './deck/LoopControls.svelte'
  import TempoReadout from './deck/TempoReadout.svelte'
  import { segment, segmentButton } from './deck/segment'

  /** The player this deck shows. Fixed for the component's life: key the deck on it to swap. */
  let { player }: { player: Player } = $props()

  const { library, decks, keybinds } = getApp()
  // svelte-ignore state_referenced_locally
  const drop = new TrackDrop(library, (track) => {
    decks.focus(player)
    void player.load(track)
  })
  // svelte-ignore state_referenced_locally
  setPlayer(player)
  /** Only the focused deck answers the keyboard, so several decks never fight over a key. */
  const focused = $derived(decks.focused === player)

  const track = $derived(player.track)
  const deviceTrack = $derived(player.track?.id.startsWith('usb:') ?? false)
  const gridEditable = $derived(library.writable && player.grid.canEdit && !deviceTrack)
  const currentBpm = $derived(bpmAt(player.position, player.displayGrid, track?.bpm ?? 0))

  $effect(() => {
    if (player.grid.editing && !gridEditable) player.grid.cancel()
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

  $effect(() => {
    if (!focused) return undefined
    return keybinds.register({ ...deckKeys(player), 'deck.gridEdit': toggleGridEdit })
  })

  $effect(() => {
    if (!focused || !player.grid.editing) return undefined
    return keybinds.register({
      'grid.addFlag': () => player.grid.addFlagAtPlayhead(),
      'grid.previousFlag': () => player.grid.jumpToFlag(-1),
      'grid.nextFlag': () => player.grid.jumpToFlag(1),
      'grid.undo': () => player.grid.undo(),
      'grid.redo': () => player.grid.redo()
    })
  })

  const gridShortcut = $derived(keybinds.bindings['deck.gridEdit'][0])
  const gridTitle = $derived(
    (deviceTrack
      ? 'USB tracks are read-only; sync to change them'
      : !library.writable
        ? 'Unlock editing to change the beatgrid'
        : player.grid.canEdit
          ? 'Edit the beatgrid'
          : 'This track has no beatgrid') + (gridShortcut ? ` (${chordLabel(gridShortcut)})` : '')
  )
</script>

<svelte:window
  onkeydown={(e) => {
    if (!focused || e.key !== 'Escape' || e.defaultPrevented) return
    if (player.grid.selectedFlagId != null) player.grid.select(null)
    else if (player.grid.editing && !player.editing) player.grid.cancel()
  }}
/>

<section
  class="relative shrink-0 border-b border-line-soft bg-surface"
  aria-label="Deck"
  ondragenter={drop.ondragenter}
  ondragover={drop.ondragover}
  ondragleave={drop.ondragleave}
  ondrop={drop.ondrop}
>
  {#if drop.over}
    <div
      class="pointer-events-none absolute inset-0 z-30 grid place-items-center rounded-[inherit] border-2 border-accent bg-accent-soft"
    >
      <span class="rounded bg-bg/80 px-2 py-1 text-[12px] font-semibold text-accent">Load on the deck</span>
    </div>
  {/if}
  <div
    class={[
      'relative border-b border-line-soft bg-[oklch(0.12_0.005_260)]',
      player.grid.editing && player.waveform ? 'h-[114px]' : 'h-[136px]'
    ]}
  >
    {#if player.waveform}
      <WaveformDetail />
    {:else}
      <div class="grid h-full place-items-center text-[12px] text-faint">
        {#if player.loading}
          <LoaderCircle class="size-4 animate-spin" />
        {:else if track}
          No analysis data for this track
        {:else}
          Select a track, or drag one here, to view its waveform
        {/if}
      </div>
    {/if}
    <WaveformToolbar />
  </div>
  {#if player.grid.editing && player.waveform}
    <TempoLane />
  {/if}

  <!--
    The mix deck's pieces, packed into two columns so nothing is left empty. On the left, the
    playing BPM and key, then the track and its clock, with play, cue and loops under them; on the
    right, the overview with the hot cues centred under it. A switch pinned at the head of the hot
    cue row swaps them for the beatgrid editor, centred in the same space so the switch never moves.
    The tempo fader leads, left of both.
  -->
  <div class="flex">
    <TempoFader />
    <div class="grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2.5 px-4 py-3">
      <div class="flex items-center gap-3">
        <TempoReadout bpm={currentBpm} />
        <TrackInfo />
      </div>
      <div class="relative h-[52px] overflow-hidden rounded bg-[oklch(0.12_0.005_260)]">
        <WaveformOverview />
      </div>

      <div class="flex items-center gap-3">
        <div class="flex items-center gap-1.5">
          <Transport />
        </div>
        <div class="flex items-center gap-1.5">
          <LoopControls />
        </div>
      </div>
      <div class="flex items-center gap-3">
        <div class={segment} role="group" aria-label="Show under the overview">
          <button
            class={segmentButton(!player.grid.editing, 'accent')}
            aria-pressed={!player.grid.editing}
            title={player.grid.editing ? 'Save the beatgrid and go back to the hot cues' : 'Hot cues'}
            onclick={() => player.grid.editing && void saveGrid()}>CUES</button
          >
          <button
            class={segmentButton(player.grid.editing, 'accent')}
            disabled={!player.grid.editing && !gridEditable}
            aria-pressed={player.grid.editing}
            title={player.grid.editing ? 'Editing the beatgrid' : gridTitle}
            onclick={() => !player.grid.editing && toggleGridEdit()}>GRID</button
          >
        </div>
        {#if player.grid.editing}
          <div class="flex min-w-0 flex-1 justify-center">
            <GridEditorBar bpm={currentBpm} onsave={saveGrid} />
          </div>
        {:else}
          <div class="flex min-w-0 flex-1 justify-center">
            <div class="grid w-full max-w-[618px] grid-cols-8 gap-1.5">
              <HotCueBar />
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>
</section>
