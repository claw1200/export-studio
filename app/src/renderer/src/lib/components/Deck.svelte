<script lang="ts">
  import { getApp } from '$lib/context'
  import { setDeckInfo, setPlayer } from '$lib/deck/context'
  import { deckKeys } from '$lib/deck/keys'
  import { useShortcut } from '$lib/deck/shortcut'
  import type { Player } from '$lib/player.svelte'
  import { DEVICE_TRACKS_READ_ONLY, isDeviceTrack } from '$lib/trackIds'
  import WaveformOverview from './WaveformOverview.svelte'
  import TempoFader from './TempoFader.svelte'
  import WaveformToolbar from './deck/WaveformToolbar.svelte'
  import GridEditorBar from './deck/GridEditorBar.svelte'
  import TempoLane from './deck/TempoLane.svelte'
  import TrackInfo from './deck/TrackInfo.svelte'
  import Transport from './deck/Transport.svelte'
  import HotCueBar from './deck/HotCueBar.svelte'
  import LoopControls from './deck/LoopControls.svelte'
  import TempoSteppers from './deck/TempoSteppers.svelte'
  import DeckShell from './deck/DeckShell.svelte'
  import { segmentGroup, segmentButton } from './deck/segment'

  // Fixed for the component's life: key the deck on it to swap.
  let { player }: { player: Player } = $props()

  const { library, decks, keybinds } = getApp()
  // svelte-ignore state_referenced_locally
  setPlayer(player)
  setDeckInfo({ label: null, shifted: false })
  const shortcut = useShortcut()
  // svelte-ignore state_referenced_locally
  const grid = player.grid
  const ownsKeyboard = $derived(decks.focused === player)

  const editingGrid = $derived(grid.editing)
  const deviceTrack = $derived(isDeviceTrack(player.track?.id))
  const gridEditable = $derived(library.writable && grid.canEdit && !deviceTrack)

  $effect(() => {
    if (editingGrid && !gridEditable) grid.cancel()
  })

  async function saveGrid(): Promise<void> {
    await grid.save()
    const saved = player.track
    if (saved && !grid.editing) library.setTrackBpm(saved.id, saved.bpm)
  }

  function beginGridEdit(): void {
    if (!editingGrid && gridEditable) grid.begin()
  }

  function toggleGridEdit(): void {
    if (editingGrid) void saveGrid()
    else beginGridEdit()
  }

  $effect(() => {
    if (!ownsKeyboard) return undefined
    return keybinds.register({ ...deckKeys(player), 'deck.gridEdit': toggleGridEdit })
  })

  $effect(() => {
    if (!ownsKeyboard || !editingGrid) return undefined
    return keybinds.register({
      'grid.addFlag': () => grid.addFlagAtPlayhead(),
      'grid.previousFlag': () => grid.jumpToFlag(-1),
      'grid.nextFlag': () => grid.jumpToFlag(1),
      'grid.undo': () => grid.undo(),
      'grid.redo': () => grid.redo()
    })
  })

  const gridTitle = $derived(
    (deviceTrack
      ? DEVICE_TRACKS_READ_ONLY
      : !library.writable
        ? 'Unlock editing to change the beatgrid'
        : grid.canEdit
          ? 'Edit the beatgrid'
          : 'This track has no beatgrid') + shortcut('deck.gridEdit')
  )
</script>

<svelte:window
  onkeydown={(e) => {
    if (!ownsKeyboard || e.key !== 'Escape' || e.defaultPrevented) return
    if (grid.selectedFlagId != null) grid.select(null)
    else if (editingGrid && !player.editing) grid.cancel()
  }}
/>

<DeckShell
  class="shrink-0 border-b border-line-soft bg-surface"
  waveformClass={editingGrid && player.waveform ? 'h-[114px]' : 'h-[136px]'}
  emptyMessage="Select a track, or drag one here, to view its waveform"
  onload={(track) => {
    decks.focus(player)
    void player.load(track)
  }}
>
  {#snippet waveformOverlay()}
    <WaveformToolbar />
  {/snippet}
  {#if editingGrid && player.waveform}
    <TempoLane />
  {/if}

  <div class="flex">
    <TempoFader />
    <div class="grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2.5 px-4 py-3">
      <div class="flex items-center gap-3">
        <Transport class="flex-col" />
        <TrackInfo size="lg" class="min-w-[360px] flex-1" />
      </div>
      <div class="relative h-[52px] overflow-hidden rounded bg-wave">
        <WaveformOverview />
      </div>

      <div class="flex items-center gap-3">
        <LoopControls />
        <div class="flex shrink-0 gap-1">
          <TempoSteppers stepperClass="w-[104px]" />
        </div>
      </div>
      <div class="flex items-center gap-3">
        <div class={segmentGroup} role="group" aria-label="Hot cues or beatgrid">
          <button
            class={segmentButton(!editingGrid, 'accent')}
            aria-pressed={!editingGrid}
            title={editingGrid ? 'Save the beatgrid and go back to the hot cues' : 'Hot cues'}
            onclick={() => {
              if (editingGrid) void saveGrid()
            }}>CUES</button
          >
          <button
            class={segmentButton(editingGrid, 'accent')}
            disabled={!editingGrid && !gridEditable}
            aria-pressed={editingGrid}
            title={editingGrid ? 'Editing the beatgrid' : gridTitle}
            onclick={beginGridEdit}>GRID</button
          >
        </div>
        <div class="flex min-w-0 flex-1 justify-center">
          {#if editingGrid}
            <GridEditorBar onsave={saveGrid} />
          {:else}
            <HotCueBar class="w-full max-w-[618px]" />
          {/if}
        </div>
      </div>
    </div>
  </div>
</DeckShell>
