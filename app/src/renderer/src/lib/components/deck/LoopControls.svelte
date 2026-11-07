<script lang="ts">
  import { getDeckInfo, getPlayer } from '$lib/deck/context'
  import { useShortcut } from '$lib/deck/shortcut'
  import { segment, segmentButton } from './segment'

  /** In, out and reloop/exit as on a CDJ, then a 4-beat loop and halving or doubling it. */
  /** With the 4-beat loop and its halving and doubling; mix mode keeps to in, out and reloop. */
  let { beatLoop = true }: { beatLoop?: boolean } = $props()

  const player = getPlayer()
  const { label } = getDeckInfo()
  const shortcut = useShortcut()
  const group = (name: string): string => (label ? `Deck ${label} ${name}` : name)

  const noTrack = $derived(!player.track || !!player.audioError)
</script>

<div class={segment} role="group" aria-label={group('loop')}>
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

{#if beatLoop}
  <div class={segment} role="group" aria-label={group('beat loop')}>
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
{/if}
