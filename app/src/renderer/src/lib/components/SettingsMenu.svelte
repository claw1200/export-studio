<script lang="ts">
  import { getApp } from '$lib/context'

  const { player } = getApp()
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
</div>
