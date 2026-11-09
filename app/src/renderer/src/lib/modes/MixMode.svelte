<script lang="ts">
  import { getApp } from '$lib/context'
  import { playOnlyKeys } from '$lib/deck/keys'
  import { chordLabel } from '$lib/keymap'
  import { Mixer } from '$lib/mixer.svelte'
  import type { Player } from '$lib/player.svelte'
  import type { Track } from '$lib/types'
  import Crossfader from '$lib/components/mix/Crossfader.svelte'
  import EqKnob from '$lib/components/mix/EqKnob.svelte'
  import MixDeck from '$lib/components/mix/MixDeck.svelte'
  import PlayButton from '$lib/components/deck/PlayButton.svelte'
  import LibraryBrowser from '$lib/components/LibraryBrowser.svelte'
  import type { ModeProps } from './types'

  /** Two decks either side of a crossfader, to hear how a transition will sound. */
  let { players, actions, inspectorOpen }: ModeProps = $props()

  const { decks, keybinds } = getApp()
  // svelte-ignore state_referenced_locally
  const [a, b] = players
  const mixer = new Mixer(a, b, decks.mixPrefs)

  const playShortcut = $derived(keybinds.bindings['mix.playBoth'][0])
  const playTitle = $derived(
    (mixer.playing ? 'Stop both decks and go back to where they started' : 'Play both decks together') +
      (playShortcut ? ` (${chordLabel(playShortcut)})` : '')
  )

  const mixKeys = (player: Player): ReturnType<typeof playOnlyKeys> =>
    playOnlyKeys(player, () => mixer.toggleDeck(player))
  $effect(() => keybinds.register(mixKeys(a)))
  $effect(() => keybinds.register(mixKeys(b), 'shift'))
  $effect(() => keybinds.register({ 'mix.playBoth': () => mixer.toggle() }))
  $effect(() => {
    decks.mixMaster = mixer.master
    return () => {
      if (decks.mixMaster === mixer.master) decks.mixMaster = null
    }
  })

  const masterLabel = $derived(mixer.label(mixer.master))
  const followerLabel = $derived(mixer.label(mixer.follower))
  const syncTitle = $derived(
    !mixer.canSync
      ? 'Load a track with a tempo on both decks to sync them'
      : mixer.synced
        ? `Deck ${followerLabel} follows deck ${masterLabel}. Click to stop syncing.`
        : `Sync deck ${followerLabel} to deck ${masterLabel}: match its tempo and line up the beats`
  )

  function load(track: Track): void {
    const target = decks.focused === b ? b : a
    void target.load(track)
    const other = target === a ? b : a
    if (!other.track) decks.focus(other)
  }
</script>

{#snippet deck(player: Player, label: string)}
  <MixDeck
    {player}
    {mixer}
    {label}
    focused={decks.focused === player}
    onfocus={() => decks.focus(player)}
    onedit={actions.openInEdit}
  />
{/snippet}

<div class="flex shrink-0 gap-2 border-b border-line-soft bg-bg p-2">
  {@render deck(a, 'A')}
  <div
    class="flex w-48 shrink-0 flex-col items-center justify-center gap-3 rounded-lg border border-line-soft bg-surface px-4 py-3"
  >
    <PlayButton
      size="lg"
      stopsInsteadOfPausing
      playing={mixer.playing}
      disabled={!mixer.playable && !mixer.playing}
      onclick={() => mixer.toggle()}
      label={mixer.playing ? 'Stop' : 'Play'}
      title={playTitle}
    />
    <div class="text-[11px] font-medium text-faint">{mixer.playing ? 'Stop' : 'Play both'}</div>
    <button
      class={[
        'h-7 w-full rounded-md text-[11px] font-bold tracking-wider transition-colors disabled:cursor-not-allowed disabled:opacity-40',
        mixer.synced
          ? 'bg-accent text-bg hover:brightness-110'
          : 'border border-line text-muted hover:bg-hover hover:text-fg'
      ]}
      disabled={!mixer.canSync}
      aria-pressed={mixer.synced}
      title={syncTitle}
      onclick={() => mixer.setSynced(!mixer.synced)}>SYNC</button
    >
    <Crossfader {mixer} />
    <button
      class={[
        'h-6 w-full rounded-md text-[10.5px] font-bold tracking-wider transition-colors',
        mixer.swapBass
          ? 'bg-accent-soft text-accent hover:brightness-110'
          : 'border border-line text-faint hover:bg-hover hover:text-muted'
      ]}
      aria-pressed={mixer.swapBass}
      title={mixer.swapBass
        ? 'Swap bass on: the deck being faded away from gives up its low end by the centre. Click to turn it off.'
        : 'Swap bass: cut the low end of the deck being faded away from as the other comes in, done by the centre'}
      onclick={() => mixer.setSwapBass(!mixer.swapBass)}>SWAP BASS</button
    >
    <div class="flex w-full justify-around">
      <EqKnob label="A" value={mixer.lowKnobs[0]} active={mixer.swapBass} />
      <EqKnob label="B" value={mixer.lowKnobs[1]} active={mixer.swapBass} />
    </div>
  </div>
  {@render deck(b, 'B')}
</div>
<LibraryBrowser {actions} {inspectorOpen} onactivate={load} settingsPrefix="mix." />
