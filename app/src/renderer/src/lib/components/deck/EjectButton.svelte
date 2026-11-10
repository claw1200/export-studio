<script lang="ts">
  import { Eject } from '@lucide/svelte'
  import type { ClassValue } from 'svelte/elements'
  import { deckName, getDeckInfo, getPlayer } from '$lib/deck/context'

  let { class: className }: { class?: ClassValue } = $props()

  const player = getPlayer()
  const name = deckName(getDeckInfo())
</script>

{#if player.track}
  <button
    class={['grid size-6 place-items-center disabled:opacity-40', className]}
    disabled={player.playing}
    aria-label="Eject {name}"
    title={player.playing ? 'Pause the deck to eject its track' : 'Eject the track'}
    onclick={() => player.eject()}
  >
    <Eject class="size-3.5" />
  </button>
{/if}
