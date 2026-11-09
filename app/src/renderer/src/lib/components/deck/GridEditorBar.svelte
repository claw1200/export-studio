<script lang="ts">
  import {
    ChevronLeft,
    ChevronRight,
    FlagTriangleRight,
    Link,
    LoaderCircle,
    Redo2,
    Undo2,
    Unlink
  } from '@lucide/svelte'
  import type { Component } from 'svelte'
  import { getPlayer } from '$lib/deck/context'
  import { useShortcut } from '$lib/deck/shortcut'
  import { VARIES_DESCRIPTION, varies } from '$lib/tempo-map'
  import BpmField from '../BpmField.svelte'
  import { segmentGroup, segmentButton } from './segment'

  let { onsave }: { onsave: () => void } = $props()

  const player = getPlayer()
  const grid = player.grid
  const shortcut = useShortcut()
  const NUDGE_MS = 1
  const SHIFT_NUDGE_MS = 10
  const nudge = (direction: -1 | 1, e: MouseEvent): void =>
    grid.nudge(direction * (e.shiftKey ? SHIFT_NUDGE_MS : NUDGE_MS))
  const nudgeTitle = (side: string): string =>
    `Shift this section ${side} by ${NUDGE_MS} ms (Shift: ${SHIFT_NUDGE_MS} ms)`

  const section = $derived(grid.map?.flags[grid.sectionAtPlayhead])
  const sectionVaries = $derived(!!section && varies(section))
  const sectionRamps = $derived(!!section && grid.sectionAtPlayhead > 0 && section.rampBeats > 0)
  const sectionBpm = $derived(section && !sectionVaries ? section.bpm : player.trackBpmAtPlayhead)
</script>

{#snippet tool(
  content: string | Component<{ class?: string }>,
  title: string,
  onclick: (e: MouseEvent) => void,
  { disabled = false, tone = 'cue' }: { disabled?: boolean; tone?: 'cue' | 'flag' } = {}
)}
  <button class={segmentButton(false, tone)} disabled={player.editing || disabled} aria-label={title} {title} {onclick}>
    {#if typeof content === 'string'}
      {content}
    {:else}
      {@const Icon = content}
      <Icon class="size-3.5" />
    {/if}
  </button>
{/snippet}

<div class="flex flex-wrap items-center gap-2" role="toolbar" aria-label="Beatgrid editor">
  <div
    class="tabular flex h-6 items-center gap-1 rounded border border-accent/60 pr-0.5 pl-2 font-mono text-[11.5px] font-semibold"
    role="group"
    aria-label="Section tempo"
  >
    <span class="font-sans text-[9px] font-semibold tracking-wide text-faint uppercase">BPM</span>
    <BpmField bpm={sectionBpm} compact />
  </div>
  {#if sectionRamps || sectionVaries}
    <span
      class="rounded-[3px] bg-flag/18 px-1 text-[9.5px] font-bold tracking-wider text-flag"
      title={sectionRamps ? 'This section ramps to its tempo' : VARIES_DESCRIPTION}
      >{sectionRamps ? 'RAMP' : 'VARIES'}</span
    >
  {/if}

  <div class={segmentGroup} role="group" aria-label="Move this section">
    {@render tool('←', nudgeTitle('left'), (e) => nudge(-1, e))}
    {@render tool('→', nudgeTitle('right'), (e) => nudge(1, e))}
    {@render tool('SET', 'Move the nearest beat to the playhead and make it the downbeat', () =>
      grid.setDownbeatHere()
    )}
  </div>
  <div class={segmentGroup} role="group" aria-label="Scale this section's tempo">
    {@render tool('/2', "Halve this section's BPM", () => grid.halve())}
    {@render tool('×2', "Double this section's BPM", () => grid.double())}
  </div>
  <div class={segmentGroup} role="group" aria-label="Tempo flags">
    {@render tool(
      FlagTriangleRight,
      'Add a tempo flag on the beat nearest the playhead' + shortcut('grid.addFlag'),
      () => grid.addFlagAtPlayhead(),
      { tone: 'flag' }
    )}
    {@render tool(ChevronLeft, 'Previous tempo flag' + shortcut('grid.previousFlag'), () => grid.jumpToFlag(-1))}
    {@render tool(ChevronRight, 'Next tempo flag' + shortcut('grid.nextFlag'), () => grid.jumpToFlag(1))}
  </div>
  <div class={segmentGroup}>
    <button
      class={segmentButton(grid.link, 'accent')}
      onclick={() => grid.setLink(!grid.link)}
      aria-pressed={grid.link}
      aria-label="Move cues with the grid"
      title={grid.link ? 'Hot cues and memory cues move with the grid' : 'Cues stay where they are when the grid moves'}
    >
      {#if grid.link}
        <Link class="size-3.5" />
      {:else}
        <Unlink class="size-3.5" />
      {/if}
    </button>
  </div>
  <div class={segmentGroup} role="group" aria-label="History">
    {@render tool(Undo2, 'Undo' + shortcut('grid.undo'), () => grid.undo(), { disabled: !grid.canUndo })}
    {@render tool(Redo2, 'Redo' + shortcut('grid.redo'), () => grid.redo(), { disabled: !grid.canRedo })}
  </div>

  <div class="flex items-center gap-1">
    <button
      class="h-6 rounded border border-line px-2 text-[10.5px] font-bold tracking-wide text-muted transition hover:bg-hover hover:text-fg disabled:opacity-40"
      disabled={player.editing}
      onclick={() => grid.cancel()}
      title="Discard grid changes (Esc)">CANCEL</button
    >
    <button
      class="flex h-6 items-center gap-1 rounded bg-accent px-2.5 text-[10.5px] font-bold tracking-wide text-bg transition hover:brightness-110 disabled:opacity-40"
      disabled={player.editing}
      onclick={onsave}
      title={(grid.dirty ? 'Save the beatgrid to rekordbox' : 'No changes to save; close the editor') +
        shortcut('deck.gridEdit')}
    >
      {#if player.editing}<LoaderCircle class="size-3 animate-spin" />{/if}
      SAVE
    </button>
  </div>
</div>
