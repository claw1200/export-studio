<script lang="ts">
  import { Play, Square } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { playKeys } from '$lib/deck/keys'
  import { chordLabel } from '$lib/keymap'
  import { Mixer } from '$lib/mixer.svelte'
  import type { Player } from '$lib/player.svelte'
  import type { Track } from '$lib/types'
  import Crossfader from '$lib/components/mix/Crossfader.svelte'
  import EqKnob from '$lib/components/mix/EqKnob.svelte'
  import MixDeck from '$lib/components/mix/MixDeck.svelte'
  import LibraryBrowser from '$lib/components/LibraryBrowser.svelte'
  import type { ModeProps } from './types'

  /**
   * Two decks either side of a crossfader, to hear how a transition will sound. Put each deck's
   * playhead where the mix should start, press play to start both at once, and fade across.
   * Stop puts both back, ready to try it again.
   */
  let { players, actions, inspectorOpen }: ModeProps = $props()

  const { decks, keybinds } = getApp()
  // svelte-ignore state_referenced_locally
  const [a, b] = players
  const mixer = new Mixer(a, b)

  const playShortcut = $derived(keybinds.bindings['mix.playBoth'][0])
  const playTitle = $derived(
    (mixer.playing ? 'Stop both decks and go back to where they started' : 'Play both decks together') +
      (playShortcut ? ` (${chordLabel(playShortcut)})` : '')
  )

  // Every deck shortcut works here as in prepare mode, bar track prep: on deck A as it is, on deck
  // B with Shift added. A deck's own play lines a synced follower up with the master.
  const mixKeys = (player: Player): ReturnType<typeof playKeys> => playKeys(player, () => mixer.toggleDeck(player))
  $effect(() => keybinds.register(mixKeys(a)))
  $effect(() => keybinds.register(mixKeys(b), 'shift'))
  $effect(() => keybinds.register({ 'mix.playBoth': () => mixer.toggle() }))
  // Keeps the follower on the master's tempo as either deck plays through tempo changes.
  $effect(() => mixer.follow())
  // A deck losing its track cannot be synced to.
  $effect(() => {
    if (mixer.synced && !mixer.canSync) mixer.setSynced(false)
  })

  const masterLabel = $derived(mixer.master === a ? 'A' : 'B')
  const syncTitle = $derived(
    !mixer.canSync
      ? 'Load a track with a tempo on both decks to sync them'
      : mixer.synced
        ? `Deck ${mixer.master === a ? 'B' : 'A'} follows deck ${masterLabel}. Click to stop syncing.`
        : `Sync deck ${mixer.master === a ? 'B' : 'A'} to deck ${masterLabel}: match its tempo and line up the beats`
  )

  /** Loads onto the focused deck, then moves the focus to the other deck if it is still empty. */
  function load(track: Track): void {
    const target = decks.focused === b ? b : a
    void target.load(track)
    const other = target === a ? b : a
    if (!other.track) decks.focus(other)
  }
</script>

{#snippet deck(player: Player, label: string)}
  <MixDeck {player} {mixer} {label} focused={decks.focused === player} onfocus={() => decks.focus(player)} />
{/snippet}

<div class="flex shrink-0 gap-2 border-b border-line-soft bg-bg p-2">
  {@render deck(a, 'A')}
  <div
    class="flex w-48 shrink-0 flex-col items-center justify-center gap-3 rounded-lg border border-line-soft bg-surface px-4 py-3"
  >
    <button
      class={[
        'grid size-14 place-items-center rounded-full shadow-lg shadow-black/40 transition disabled:cursor-not-allowed disabled:opacity-40',
        mixer.playing ? 'bg-accent text-bg hover:brightness-110' : 'bg-raised text-fg hover:bg-hover'
      ]}
      disabled={!mixer.ready && !mixer.playing}
      onclick={() => mixer.toggle()}
      aria-label={mixer.playing ? 'Stop' : 'Play'}
      title={playTitle}
    >
      {#if mixer.playing}
        <Square class="size-5" fill="currentColor" />
      {:else}
        <Play class="ml-0.5 size-6" fill="currentColor" />
      {/if}
    </button>
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
      <EqKnob label="A" value={mixer.lows[0]} active={mixer.swapBass} />
      <EqKnob label="B" value={mixer.lows[1]} active={mixer.swapBass} />
    </div>
  </div>
  {@render deck(b, 'B')}
</div>
<LibraryBrowser {actions} {inspectorOpen} onactivate={load} settingsPrefix="mix." />
