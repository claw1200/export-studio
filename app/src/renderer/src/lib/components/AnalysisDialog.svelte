<script lang="ts">
  import { AudioWaveform, X } from '@lucide/svelte'
  import { BPM_RANGES, MAX_CONCURRENCY, type AnalysisSettings } from '$lib/analysis.svelte'
  import { getApp } from '$lib/context'
  import { suspendModal } from '$lib/modal'
  import type { Track } from '$lib/types'
  import type { GridMode } from '../../../../preload/analysis'

  let {
    tracks,
    onrun,
    onclose
  }: {
    tracks: Track[]
    onrun: (tracks: Track[], settings: AnalysisSettings) => void
    onclose: () => void
  } = $props()

  const { analysis, keybinds } = getApp()
  let draft = $state<AnalysisSettings>({ ...analysis.settings })

  const GRID_MODES: { id: GridMode; label: string }[] = [
    { id: 'normal', label: 'Normal' },
    { id: 'dynamic', label: 'Dynamic' }
  ]
  const BPM_OPTIONS = BPM_RANGES.map(([min, max]) => ({ id: `${min}-${max}`, label: `${min}-${max}` }))
  const CONCURRENCY = Array.from({ length: MAX_CONCURRENCY }, (_, index) => String(index + 1))

  function setRange(value: string): void {
    const [minBpm, maxBpm] = value.split('-').map(Number)
    draft.minBpm = minBpm
    draft.maxBpm = maxBpm
  }

  function run(dialog: HTMLDialogElement): void {
    onrun(tracks, { ...draft })
    dialog.close()
  }
</script>

<dialog
  {@attach suspendModal(keybinds)}
  class="no-drag m-auto max-h-[calc(100vh-48px)] w-[560px] max-w-[calc(100vw-48px)] overflow-hidden rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Analysis options"
  onclose={() => onclose()}
  onclick={(e) => {
    if (e.target === e.currentTarget) e.currentTarget.close()
  }}
>
  <div class="flex max-h-[calc(100vh-48px)] flex-col">
    <header class="flex shrink-0 items-center gap-3 border-b border-line-soft px-5 py-3.5">
      <div class="grid size-8 shrink-0 place-items-center rounded-md bg-raised text-accent">
        <AudioWaveform class="size-4" />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="text-[13.5px] font-semibold">Analysis options</h2>
        <p class="text-[11.5px] text-faint">{tracks.length} {tracks.length === 1 ? 'track' : 'tracks'}</p>
      </div>
      <button
        type="button"
        class="grid size-7 shrink-0 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg"
        onclick={(e) => e.currentTarget.closest('dialog')?.close()}
        aria-label="Close analysis options"
      >
        <X class="size-4" />
      </button>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      <section class="mb-5">
        <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">Beatgrid</h3>
        <div class="flex items-start gap-3 rounded-md px-2 py-2">
          <span class="min-w-0 flex-1">
            <span class="block text-[12.5px] font-medium">Grid</span>
            <span class="mt-0.5 block text-[11px] leading-snug text-faint">
              Normal fits one tempo to the track. Dynamic follows tempo changes.
            </span>
          </span>
          <div class="flex shrink-0 rounded-md border border-line bg-bg p-0.5 text-[11px] font-semibold" role="radiogroup" aria-label="Grid">
            {#each GRID_MODES as mode (mode.id)}
              <button
                type="button"
                class={[
                  'rounded px-2.5 py-1',
                  draft.gridMode === mode.id ? 'bg-raised text-fg' : 'text-faint hover:text-muted'
                ]}
                role="radio"
                aria-checked={draft.gridMode === mode.id}
                onclick={() => (draft.gridMode = mode.id)}
              >
                {mode.label}
              </button>
            {/each}
          </div>
        </div>

        <label class="flex items-start gap-3 rounded-md px-2 py-2">
          <span class="min-w-0 flex-1">
            <span class="block text-[12.5px] font-medium">BPM range</span>
            <span class="mt-0.5 block text-[11px] leading-snug text-faint">
              The tempo is picked inside this range.
            </span>
          </span>
          <select
            class="h-7 shrink-0 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
            aria-label="BPM range"
            value={`${draft.minBpm}-${draft.maxBpm}`}
            onchange={(e) => setRange(e.currentTarget.value)}
          >
            {#each BPM_OPTIONS as option (option.id)}
              <option value={option.id}>{option.label}</option>
            {/each}
          </select>
        </label>

        {@render toggle(
          'High precision',
          'Place each beat on the start of a kick or snare.',
          draft.highPrecision,
          () => (draft.highPrecision = !draft.highPrecision)
        )}
        {@render toggle(
          'Round BPM',
          'Snap tempos very close to a whole number.',
          draft.roundBpm,
          () => (draft.roundBpm = !draft.roundBpm)
        )}
      </section>

      <section>
        <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">More</h3>
        {@render toggle('Detect key', 'Analyse the musical key as well as the beatgrid.', draft.key, () => (draft.key = !draft.key))}
        <label class="flex items-start gap-3 rounded-md px-2 py-2">
          <span class="min-w-0 flex-1">
            <span class="block text-[12.5px] font-medium">Tracks analysed at once</span>
            <span class="mt-0.5 block text-[11px] leading-snug text-faint">
              More is faster on machines with many cores, but leaves less for playback.
            </span>
          </span>
          <select
            class="h-7 w-16 shrink-0 rounded-md border border-line bg-bg px-2 text-[12px] text-fg"
            aria-label="Tracks analysed at once"
            value={String(draft.concurrency)}
            onchange={(e) => (draft.concurrency = Number(e.currentTarget.value))}
          >
            {#each CONCURRENCY as count (count)}
              <option value={count}>{count}</option>
            {/each}
          </select>
        </label>
      </section>
    </div>

    <footer class="flex shrink-0 justify-end gap-2 border-t border-line-soft px-5 py-3">
      <button
        type="button"
        class="h-7 rounded-md px-3 text-[12.5px] text-muted hover:bg-hover hover:text-fg"
        onclick={(e) => e.currentTarget.closest('dialog')?.close()}
      >
        Cancel
      </button>
      <button
        type="button"
        class="flex h-7 items-center gap-1.5 rounded-md bg-accent px-3 text-[12.5px] font-medium text-bg hover:bg-accent/90"
        onclick={(e) => {
          const dialog = e.currentTarget.closest('dialog')
          if (dialog) run(dialog)
        }}
      >
        <AudioWaveform class="size-3.5" />
        Analyse
      </button>
    </footer>
  </div>
</dialog>

{#snippet toggle(label: string, description: string, on: boolean, flip: () => void)}
  <button
    type="button"
    class="flex w-full items-start gap-3 rounded-md px-2 py-2 text-left hover:bg-hover"
    role="switch"
    aria-checked={on}
    onclick={flip}
  >
    <span class="min-w-0 flex-1">
      <span class="block text-[12.5px] font-medium">{label}</span>
      <span class="mt-0.5 block text-[11px] leading-snug text-faint">{description}</span>
    </span>
    <span
      class={['relative mt-0.5 h-[18px] w-8 shrink-0 rounded-full transition-colors', on ? 'bg-accent' : 'bg-line']}
      aria-hidden="true"
    >
      <span
        class={[
          'absolute top-0.5 left-0.5 size-3.5 rounded-full bg-white shadow transition-transform',
          on && 'translate-x-3.5'
        ]}
      ></span>
    </span>
  </button>
{/snippet}