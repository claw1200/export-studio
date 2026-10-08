<script lang="ts">
  import { Minus, Plus } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import type { WaveformMode } from '$lib/waveform/data'

  const { player } = getApp()
  const display = player.display

  const MODES: { id: WaveformMode; label: string }[] = [
    { id: 'rgb', label: 'RGB' },
    { id: '3band', label: '3-Band' }
  ]
</script>

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
    <button
      class="grid size-6 place-items-center text-faint hover:text-fg"
      onclick={() => display.setZoom(display.zoom / 1.5)}
      aria-label="Zoom out"
    >
      <Minus class="size-3" />
    </button>
    <button
      class="grid size-6 place-items-center text-faint hover:text-fg"
      onclick={() => display.setZoom(display.zoom * 1.5)}
      aria-label="Zoom in"
    >
      <Plus class="size-3" />
    </button>
  </div>
</div>
