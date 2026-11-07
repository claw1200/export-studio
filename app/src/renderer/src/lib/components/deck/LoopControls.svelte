<script lang="ts">
  import { getApp } from '$lib/context'
  import { getPlayer } from '$lib/deck/context'
  import { chordLabel, type ActionId } from '$lib/keymap'
  import { segment, segmentButton } from './segment'

  /** In, out and reloop/exit as on the mix deck, then beat loop and resize. */
  const { keybinds } = getApp()
  const player = getPlayer()

  const noTrack = $derived(!player.track || !!player.audioError)
  const shortcut = (id: ActionId): string => {
    const chord = keybinds.bindings[id][0]
    return chord ? ` (${chordLabel(chord)})` : ''
  }
</script>

<div class={segment} role="group" aria-label="Loop">
  <button
    class={segmentButton()}
    disabled={noTrack}
    onclick={() => player.setLoopIn()}
    title="Loop in{shortcut('deck.loopIn')}">IN</button
  >
  <button
    class={segmentButton()}
    disabled={noTrack}
    onclick={() => player.setLoopOut()}
    title="Loop out{shortcut('deck.loopOut')}">OUT</button
  >
  <button
    class={segmentButton(!!player.loop?.active)}
    disabled={!player.loop}
    onclick={() => player.reloopExit()}
    aria-pressed={!!player.loop?.active}
    title="{player.loop?.active ? 'Exit the loop' : 'Back into the loop'}{shortcut('deck.reloopExit')}"
  >
    <!-- Both labels share one cell, so the button is as wide as the longer and keeps its width. -->
    <span class={['col-start-1 row-start-1', !player.loop?.active && 'invisible']}>EXIT</span>
    <span class={['col-start-1 row-start-1', player.loop?.active && 'invisible']}>RELOOP</span>
  </button>
</div>

<div class={segment} role="group" aria-label="Beat loop">
  <button
    class={segmentButton()}
    disabled={noTrack}
    onclick={() => player.beatLoop(4)}
    title="4-beat loop{shortcut('deck.beatLoop')}">4 BEAT</button
  >
  <button
    class={segmentButton()}
    disabled={!player.loop}
    onclick={() => player.resizeLoop(0.5)}
    aria-label="Halve loop"
    title="Halve loop{shortcut('deck.loopHalve')}">½</button
  >
  <button
    class={segmentButton()}
    disabled={!player.loop}
    onclick={() => player.resizeLoop(2)}
    aria-label="Double loop"
    title="Double loop{shortcut('deck.loopDouble')}">×2</button
  >
</div>
