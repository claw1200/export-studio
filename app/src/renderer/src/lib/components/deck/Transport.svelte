<script lang="ts">
  import { Pause, Play } from '@lucide/svelte'
  import { getDeckInfo, getPlayer } from '$lib/deck/context'
  import { deckName, useShortcut } from '$lib/deck/shortcut'

  /** Play and cue. Mix mode plays through the mixer, so a follower lines up with the master. */
  let {
    onplay,
    playTitle
  }: {
    /** In place of the player's own play/pause. */
    onplay?: () => void
    /** What play does, when it is more than playing. */
    playTitle?: string
  } = $props()

  const player = getPlayer()
  const { label } = getDeckInfo()
  const shortcut = useShortcut()
  const name = deckName(label)

  const disabled = $derived(!player.track || !!player.audioError)
  const cueLit = $derived(player.cueHeld || (!player.playing && !!player.track && player.atCue))
  const cueTitle = $derived(
    (player.playing
      ? 'Return to the cue point and stop'
      : player.atCue
        ? 'Hold to play from the cue point'
        : `Set the cue point here${player.quantise ? ', on the nearest beat' : ''}`) + shortcut('deck.cue')
  )
</script>

<div class="flex shrink-0 items-center gap-1.5">
  <button
    class={[
      'grid size-7 place-items-center rounded-full transition disabled:opacity-30',
      player.playing ? 'bg-accent text-bg hover:brightness-110' : 'bg-raised text-fg hover:bg-hover'
    ]}
    {disabled}
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
    {disabled}
    onpointerdown={(e) => {
      if (e.button !== 0) return
      e.currentTarget.setPointerCapture(e.pointerId)
      player.cueDown()
    }}
    onpointerup={() => player.cueUp()}
    onpointercancel={() => player.cueUp()}
    onlostpointercapture={() => player.cueUp()}
    aria-label="Cue {name}"
    title={cueTitle}
  >
    CUE
  </button>
</div>
