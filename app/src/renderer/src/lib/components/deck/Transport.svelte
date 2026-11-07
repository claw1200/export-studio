<script lang="ts">
  import { Pause, Play } from '@lucide/svelte'
  import { deckName, getDeckInfo, getPlayer } from '$lib/deck/context'
  import { hold } from '$lib/deck/hold'
  import { useShortcut } from '$lib/deck/shortcut'

  let {
    onplay,
    playTitle
  }: {
    /** Replaces the player's own play/pause, as the mixer does to keep a follower in sync. */
    onplay?: () => void
    playTitle?: string
  } = $props()

  const player = getPlayer()
  const name = deckName(getDeckInfo())
  const shortcut = useShortcut()

  const cueLit = $derived(player.cueHeld || (!player.playing && !!player.track && player.atCue))
  const cueTitle = $derived(
    player.playing
      ? 'Return to the cue point and stop'
      : player.atCue
        ? 'Hold to play from the cue point'
        : `Set the cue point here${player.quantise ? ', on the nearest beat' : ''}`
  )
</script>

<div class="flex shrink-0 items-center gap-1.5">
  <button
    class={[
      'grid size-7 place-items-center rounded-full transition disabled:opacity-30',
      player.playing ? 'bg-accent text-bg hover:brightness-110' : 'bg-raised text-fg hover:bg-hover'
    ]}
    disabled={!player.playable}
    onclick={() => (onplay ? onplay() : player.toggle())}
    aria-label={player.playing ? `Pause ${name}` : `Play ${name}`}
    title={(player.playing ? `Pause ${name}` : (playTitle ?? `Play ${name}`)) + shortcut('deck.playPause')}
  >
    {#if player.playing}
      <Pause class="size-3.5 fill-current" />
    {:else}
      <Play class="ml-px size-3.5 fill-current" />
    {/if}
  </button>
  <button
    class={[
      'h-6 w-11 touch-none rounded-full border text-[10.5px] font-bold tracking-wide transition select-none disabled:opacity-30',
      cueLit
        ? 'border-transparent bg-cue text-bg shadow-[0_0_10px] shadow-cue/50'
        : 'border-cue/60 text-cue hover:bg-cue/15'
    ]}
    disabled={!player.playable}
    {@attach hold(
      () => player.cueDown(),
      () => player.cueUp()
    )}
    aria-label="Cue {name}"
    title={cueTitle + shortcut('deck.cue')}
  >
    CUE
  </button>
</div>
