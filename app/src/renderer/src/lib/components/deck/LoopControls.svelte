<script lang="ts">
  import { getPlayer } from '$lib/deck/context'
  import { useShortcut } from '$lib/deck/shortcut'
  import { BEAT_LOOP_BEATS } from '$lib/deck/transport'
  import { segmentButton, segmentGroup } from './segment'

  let { showBeatLoop = true }: { showBeatLoop?: boolean } = $props()

  const player = getPlayer()
  const shortcut = useShortcut()
  const looping = $derived(!!player.loop?.active)
</script>

<div class="flex items-center gap-1.5">
  <div class={segmentGroup} role="group" aria-label="Loop">
    <button
      class={segmentButton()}
      disabled={!player.playable}
      onclick={() => player.setLoopIn()}
      title="Loop in{shortcut('deck.loopIn')}">IN</button
    >
    <button
      class={segmentButton()}
      disabled={!player.playable}
      onclick={() => player.setLoopOut()}
      title="Loop out{shortcut('deck.loopOut')}">OUT</button
    >
    <button
      class={segmentButton(looping)}
      disabled={!player.loop}
      onclick={() => player.reloopExit()}
      aria-pressed={looping}
      title="{looping ? 'Exit the loop' : 'Back into the loop'}{shortcut('deck.reloopExit')}"
    >
      <!-- Both labels share one cell so the button keeps the longer one's width. -->
      <span class={['col-start-1 row-start-1', !looping && 'invisible']}>EXIT</span>
      <span class={['col-start-1 row-start-1', looping && 'invisible']}>RELOOP</span>
    </button>
  </div>

  {#if showBeatLoop}
    <div class={segmentGroup} role="group" aria-label="Beat loop">
      <button
        class={segmentButton()}
        disabled={!player.playable}
        onclick={() => player.beatLoop(BEAT_LOOP_BEATS)}
        title="{BEAT_LOOP_BEATS}-beat loop{shortcut('deck.beatLoop')}">{BEAT_LOOP_BEATS} BEAT</button
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
</div>
