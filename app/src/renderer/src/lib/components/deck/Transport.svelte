<script lang="ts">
  import { Pause, Play } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { getPlayer } from '$lib/deck/context'
  import { chordLabel, type ActionId } from '$lib/keymap'

  /** Play and cue, sized and lit as on the mix deck. */
  const { keybinds } = getApp()
  const player = getPlayer()

  const disabled = $derived(!player.track || !!player.audioError)
  const cueLit = $derived(player.cueHeld || (!player.playing && !!player.track && player.atCue))
  const shortcut = (id: ActionId): string => {
    const chord = keybinds.bindings[id][0]
    return chord ? ` (${chordLabel(chord)})` : ''
  }
  const cueTitle = $derived(
    (player.playing
      ? 'Return to the cue point and stop'
      : player.atCue
        ? 'Hold to play from the cue point'
        : `Set the cue point here${player.quantise ? ', on the nearest beat' : ''}`) + shortcut('deck.cue')
  )
</script>

<button
  class={[
    'grid size-7 shrink-0 place-items-center rounded-full transition disabled:opacity-30',
    player.playing ? 'bg-accent text-bg hover:brightness-110' : 'bg-raised text-fg hover:bg-hover'
  ]}
  {disabled}
  onclick={() => player.toggle()}
  aria-label={player.playing ? 'Pause' : 'Play'}
  title={(player.playing ? 'Pause' : 'Play') + shortcut('deck.playPause')}
>
  {#if player.playing}
    <Pause class="size-3.5 fill-current" />
  {:else}
    <Play class="ml-px size-3.5 fill-current" />
  {/if}
</button>
<button
  class={[
    'h-6 w-11 shrink-0 touch-none rounded-full border text-[10.5px] font-bold tracking-wide transition select-none disabled:opacity-30',
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
  aria-label="Cue"
  title={cueTitle}
>
  CUE
</button>
