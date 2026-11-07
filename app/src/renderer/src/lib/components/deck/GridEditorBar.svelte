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
  import { getApp } from '$lib/context'
  import { getPlayer } from '$lib/deck/context'
  import { chordLabel } from '$lib/keymap'
  import { varies } from '$lib/tempo-map'
  import BpmField from '../BpmField.svelte'
  import { segment, segmentButton } from './segment'

  /**
   * The beatgrid editor, in place of the hot cues while it is open: the BPM of the section at the
   * playhead, to drag or type, then its tools, ending in cancel and save so they sit with the
   * tools rather than across the deck.
   */
  const {
    bpm,
    onsave
  }: {
    /** At the playhead, at the track's own tempo: shown where the section's tempo varies. */
    bpm: number
    onsave: () => void
  } = $props()

  const { keybinds } = getApp()
  const player = getPlayer()
  const grid = player.grid

  const flag = $derived(grid.map?.flags[grid.sectionAtPlayhead])
  const sectionChip = $derived(
    flag && grid.sectionAtPlayhead > 0 && flag.rampBeats > 0
      ? { label: 'RAMP', title: 'This section ramps to its tempo' }
      : flag && varies(flag)
        ? { label: 'VARIES', title: 'The tempo here varies beat by beat, as rekordbox analysed it' }
        : null
  )
  const sectionBpm = $derived(flag && !varies(flag) ? flag.bpm : bpm)
  const gridShortcut = $derived(keybinds.bindings['deck.gridEdit'][0])
</script>

{#snippet tool(label: string, title: string, onclick: (e: MouseEvent) => void, disabled = false)}
  <button class={segmentButton()} disabled={player.editing || disabled} aria-label={title} {title} {onclick}
    >{label}</button
  >
{/snippet}

{#snippet iconTool(
  Icon: Component<{ class?: string }>,
  title: string,
  onclick: () => void,
  disabled = false,
  flagged = false
)}
  <button
    class={segmentButton(false, flagged ? 'flag' : 'cue')}
    disabled={player.editing || disabled}
    aria-label={title}
    {title}
    {onclick}
  >
    <Icon class="size-3.5" />
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
  {#if sectionChip}
    <span
      class="rounded-[3px] bg-flag/18 px-1 text-[9.5px] font-bold tracking-wider text-flag"
      title={sectionChip.title}>{sectionChip.label}</span
    >
  {/if}
  <div class={segment} role="group" aria-label="Move this section">
    {@render tool('←', 'Shift this section left by 1 ms (Shift: 10 ms)', (e) => grid.nudge(e.shiftKey ? -10 : -1))}
    {@render tool('→', 'Shift this section right by 1 ms (Shift: 10 ms)', (e) => grid.nudge(e.shiftKey ? 10 : 1))}
    {@render tool('SET', 'Move the nearest beat to the playhead and make it the downbeat', () =>
      grid.setDownbeatHere()
    )}
  </div>
  <div class={segment} role="group" aria-label="Scale this section's tempo">
    {@render tool('/2', "Halve this section's BPM", () => grid.halve())}
    {@render tool('×2', "Double this section's BPM", () => grid.double())}
  </div>
  <div class={segment} role="group" aria-label="Tempo flags">
    {@render iconTool(
      FlagTriangleRight,
      'Add a tempo flag on the beat nearest the playhead (T)',
      () => grid.addFlagAtPlayhead(),
      false,
      true
    )}
    {@render iconTool(ChevronLeft, 'Previous tempo flag (,)', () => grid.jumpToFlag(-1))}
    {@render iconTool(ChevronRight, 'Next tempo flag (.)', () => grid.jumpToFlag(1))}
  </div>
  <div class={segment}>
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
  <div class={segment} role="group" aria-label="History">
    {@render iconTool(Undo2, 'Undo (⌘Z)', () => grid.undo(), !grid.canUndo)}
    {@render iconTool(Redo2, 'Redo (⇧⌘Z)', () => grid.redo(), !grid.canRedo)}
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
        (gridShortcut ? ` (${chordLabel(gridShortcut)})` : '')}
    >
      {#if player.editing}<LoaderCircle class="size-3 animate-spin" />{/if}
      SAVE
    </button>
  </div>
</div>
