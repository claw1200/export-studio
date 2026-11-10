<script lang="ts">
  import { Minus, Plus } from '@lucide/svelte'
  import type { Component } from 'svelte'
  import { getPlayer } from '$lib/deck/context'
  import type { WaveformMode } from '$lib/waveform/data'
  import EjectButton from './EjectButton.svelte'

  const player = getPlayer()
  const display = player.display

  const ZOOM_STEP = 1.5
  const MODES: { id: WaveformMode; label: string }[] = [
    { id: 'rgb', label: 'Color' },
    { id: '3band', label: '3-Band' }
  ]
</script>

{#snippet zoomButton(label: string, Icon: Component<{ class?: string }>, factor: number)}
  <button
    class="grid size-6 place-items-center text-faint hover:text-fg"
    onclick={() => display.setZoom(display.zoom * factor)}
    aria-label={label}
  >
    <Icon class="size-3" />
  </button>
{/snippet}

<div class="absolute top-2 right-2 flex items-center gap-1.5">
  <div class="flex rounded-md border border-line bg-bg/80 p-0.5 text-[10.5px] font-semibold backdrop-blur">
    {#each MODES as m (m.id)}
      <button
        class={['rounded px-2 py-0.5', display.mode === m.id ? 'bg-raised text-fg' : 'text-faint hover:text-muted']}
        onclick={() => display.setMode(m.id)}
      >
        {m.label}
      </button>
    {/each}
  </div>
  <div class="flex items-center rounded-md border border-line bg-bg/80 backdrop-blur">
    {@render zoomButton('Zoom out', Minus, 1 / ZOOM_STEP)}
    {@render zoomButton('Zoom in', Plus, ZOOM_STEP)}
  </div>
  <EjectButton class="rounded-md border border-line bg-bg/80 text-faint backdrop-blur enabled:hover:text-fg" />
</div>
