<script lang="ts">
  import { getApp } from '$lib/context'
  import Deck from '$lib/components/Deck.svelte'
  import LibraryBrowser from '$lib/components/LibraryBrowser.svelte'
  import type { Track } from '$lib/types'
  import type { ModeProps } from './types'

  let { players, actions, inspectorOpen }: ModeProps = $props()

  const { library } = getApp()
  const player = $derived(players[0])
  let previewTimer: ReturnType<typeof setTimeout> | undefined

  $effect(() => () => clearTimeout(previewTimer))

  function preview(track: Track): void {
    if (player.playing || library.doubleClickToLoad) return
    clearTimeout(previewTimer)
    previewTimer = setTimeout(() => player.load(track), 80)
  }

  function open(track: Track): void {
    clearTimeout(previewTimer)
    void player.loadAndPlay(track)
  }
</script>

{#key player}
  <Deck {player} />
{/key}
<LibraryBrowser {actions} {inspectorOpen} onselect={preview} onactivate={open} />
