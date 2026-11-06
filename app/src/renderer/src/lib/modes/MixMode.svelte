<script lang="ts">
  import { Play, Square } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { chordLabel } from '$lib/keymap'
  import { Mixer } from '$lib/mixer.svelte'
  import type { Player } from '$lib/player.svelte'
  import type { Track } from '$lib/types'
  import Crossfader from '$lib/components/mix/Crossfader.svelte'
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

  const playShortcut = $derived(keybinds.bindings['deck.playPause'][0])
  const playTitle = $derived(
    (mixer.playing ? 'Stop both decks and go back to where they started' : 'Play both decks together') +
      (playShortcut ? ` (${chordLabel(playShortcut)})` : '')
  )

  $effect(() => keybinds.register({ 'deck.playPause': () => mixer.toggle() }))

  /** Loads onto the focused deck, then moves the focus to the other deck if it is still empty. */
  function load(track: Track): void {
    const target = decks.focused === b ? b : a
    void target.load(track)
    const other = target === a ? b : a
    if (!other.track) decks.focus(other)
  }
</script>

{#snippet deck(player: Player, label: string)}
  <MixDeck {player} {label} focused={decks.focused === player} onfocus={() => decks.focus(player)} />
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
    <Crossfader {mixer} />
  </div>
  {@render deck(b, 'B')}
</div>
<LibraryBrowser {actions} {inspectorOpen} onactivate={load} settingsPrefix="mix." />
