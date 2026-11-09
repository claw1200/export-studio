<script lang="ts">
  import type { ClassValue } from 'svelte/elements'
  import DeckButton from './DeckButton.svelte'
  import PlayButton from './PlayButton.svelte'
  import { deckName, getDeckInfo, getPlayer } from '$lib/deck/context'
  import { hold } from '$lib/deck/hold'
  import { useShortcut } from '$lib/deck/shortcut'

  let {
    class: className,
    playTitle
  }: {
    /** Lays the buttons out, as `flex-col` to stack cue under play. */
    class?: ClassValue
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

<div class={['flex shrink-0 items-center gap-1.5', className]}>
  <PlayButton
    playing={player.playing}
    disabled={!player.playable}
    onclick={() => player.toggle()}
    label={player.playing ? `Pause ${name}` : `Play ${name}`}
    title={(player.playing ? `Pause ${name}` : (playTitle ?? `Play ${name}`)) + shortcut('deck.playPause')}
  />
  <DeckButton
    tone="cue"
    lit={cueLit}
    disabled={!player.playable}
    {@attach hold(
      () => player.cueDown(),
      () => player.cueUp()
    )}
    aria-label="Cue {name}"
    title={cueTitle + shortcut('deck.cue')}
  >
    <span class="text-[9px] font-bold tracking-wide drop-shadow-sm">CUE</span>
  </DeckButton>
</div>
