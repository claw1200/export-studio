<script lang="ts">
  import { getApp } from '$lib/context'
  import type { CuePlacement } from '$lib/player.svelte'

  const { player } = getApp()

  const PLACEMENTS: { id: CuePlacement; label: string }[] = [
    { id: 'playhead', label: 'Playhead' },
    { id: 'cuePoint', label: 'Cue point' }
  ]
</script>

<div
  class="absolute top-[calc(100%+6px)] right-0 z-50 w-72 rounded-lg border border-line bg-surface p-3 shadow-2xl shadow-black/60"
  role="dialog"
  aria-label="Settings"
>
  <div class="px-1.5 pb-2 text-[12px] font-semibold text-fg">Settings</div>

  <button
    class="flex w-full items-start gap-3 rounded-md px-1.5 py-1.5 text-left hover:bg-hover"
    role="switch"
    aria-checked={player.hqRgb}
    onclick={() => player.setHqRgb(!player.hqRgb)}
  >
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium text-fg">High quality RGB waveforms</span>
      <span class="mt-0.5 block text-[11px] leading-snug text-faint">
        Draw the detail waveform from the audio. Off uses the block style.
      </span>
    </span>
    <span
      class={[
        'relative mt-0.5 h-[18px] w-8 shrink-0 rounded-full transition-colors',
        player.hqRgb ? 'bg-accent' : 'bg-line'
      ]}
      aria-hidden="true"
    >
      <span
        class={[
          'absolute top-0.5 left-0.5 size-3.5 rounded-full bg-white shadow transition-transform',
          player.hqRgb && 'translate-x-3.5'
        ]}
      ></span>
    </span>
  </button>

  <div class="flex items-start gap-3 px-1.5 py-1.5">
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium text-fg">Place new cues at</span>
      <span class="mt-0.5 block text-[11px] leading-snug text-faint">Where new hot cues and memory cues go.</span>
    </span>
    <div class="flex shrink-0 rounded-md border border-line bg-bg p-0.5 text-[10.5px] font-semibold" role="radiogroup">
      {#each PLACEMENTS as p (p.id)}
        <button
          class={[
            'rounded px-2 py-0.5',
            player.cuePlacement === p.id ? 'bg-raised text-fg' : 'text-faint hover:text-muted'
          ]}
          role="radio"
          aria-checked={player.cuePlacement === p.id}
          onclick={() => player.setCuePlacement(p.id)}
        >
          {p.label}
        </button>
      {/each}
    </div>
  </div>
</div>
