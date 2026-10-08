<script lang="ts">
  import { Pause, Play } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { formatClock } from '$lib/format'
  import { chordLabel } from '$lib/keymap'

  const { player, keybinds } = getApp()

  const disabled = $derived(!player.track || !!player.audioError)
  const cueLit = $derived(player.cueHeld || (!player.playing && !!player.track && player.atCue))
  const cueShortcut = $derived(keybinds.bindings['deck.cue'][0])
  const cueTitle = $derived(
    (player.playing
      ? 'Return to the cue point and stop'
      : player.atCue
        ? 'Hold to play from the cue point'
        : `Set the cue point here${player.quantise ? ', on the nearest beat' : ''}`) +
      (cueShortcut ? ` (${chordLabel(cueShortcut)})` : '')
  )
</script>

<div class="tabular w-24 shrink-0 text-right font-mono">
  <div class="text-[15px] font-medium text-fg">{formatClock(player.position * 1000)}</div>
  <div class="text-[11px] text-faint">−{formatClock((player.duration - player.position) * 1000)}</div>
</div>

<div class="flex shrink-0 flex-col items-center gap-1.5">
  <button
    class="grid size-11 place-items-center rounded-full bg-accent text-bg transition hover:brightness-110 disabled:opacity-30"
    {disabled}
    onclick={() => player.toggle()}
    aria-label={player.playing ? 'Pause' : 'Play'}
  >
    {#if player.playing}
      <Pause class="size-5 fill-current" />
    {:else}
      <Play class="ml-0.5 size-5 fill-current" />
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
    aria-label="Cue"
    title={cueTitle}
  >
    CUE
  </button>
</div>
