<script lang="ts">
  import { getApp } from '$lib/context'
  import { setDeckInfo, setPlayer } from '$lib/deck/context'
  import { deckKeys } from '$lib/deck/keys'
  import { useShortcut } from '$lib/deck/shortcut'
  import type { Player } from '$lib/player.svelte'
  import { bpmAt } from '$lib/beat'
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
  import DeckShell from './deck/DeckShell.svelte'
  import { segment, segmentButton } from './deck/segment'

  /** The player this deck shows. Fixed for the component's life: key the deck on it to swap. */
  let { player }: { player: Player } = $props()

  const { library, decks, keybinds } = getApp()
  // svelte-ignore state_referenced_locally
  setPlayer(player)
  setDeckInfo({ label: null, shifted: false })
  const shortcut = useShortcut()
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

  const gridTitle = $derived(
    (deviceTrack
      ? 'USB tracks are read-only; sync to change them'
      : !library.writable
        ? 'Unlock editing to change the beatgrid'
        : player.grid.canEdit
          ? 'Edit the beatgrid'
          : 'This track has no beatgrid') + shortcut('deck.gridEdit')
  )
</script>

<svelte:window
  onkeydown={(e) => {
    if (!focused || e.key !== 'Escape' || e.defaultPrevented) return
    if (player.grid.selectedFlagId != null) player.grid.select(null)
    else if (player.grid.editing && !player.editing) player.grid.cancel()
  }}
/>

<DeckShell
  class="shrink-0 border-b border-line-soft bg-surface"
  waveClass={player.grid.editing && player.waveform ? 'h-[114px]' : 'h-[136px]'}
  empty="Select a track, or drag one here, to view its waveform"
  onload={(track) => {
    decks.focus(player)
    void player.load(track)
  }}
>
  {#snippet overlay()}
    <WaveformToolbar />
  {/snippet}
  {#if player.grid.editing && player.waveform}
    <TempoLane />
  {/if}

  <!--
    The pieces it shares with the mix deck, packed into two columns so nothing is left empty. On the left, the
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
        <TrackInfo class="w-[360px] shrink-0" />
      </div>
      <div class="relative h-[52px] overflow-hidden rounded bg-wave">
        <WaveformOverview />
      </div>

      <div class="flex items-center gap-3">
        <Transport />
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
</DeckShell>
