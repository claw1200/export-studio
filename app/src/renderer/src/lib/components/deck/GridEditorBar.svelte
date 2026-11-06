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
  import { varies } from '$lib/tempo-map'
  import BpmField from '../BpmField.svelte'

  const { bpm: playheadBpm, onsave }: { bpm: number; onsave: () => void } = $props()

  const player = getPlayer()
  const grid = player.grid

  const flag = $derived(grid.map?.flags[grid.sectionAtPlayhead])
  const sectionVaries = $derived(!!flag && varies(flag))
  const sectionRamps = $derived(!!flag && grid.sectionAtPlayhead > 0 && flag.rampBeats > 0)
  const sectionBpm = $derived(flag && !sectionVaries ? flag.bpm : playheadBpm)
  const sectionChip = $derived(
    sectionRamps
      ? { label: 'RAMP', title: 'This section ramps to this tempo' }
      : sectionVaries
        ? { label: 'VARIES', title: 'The tempo here varies beat by beat, as rekordbox analysed it' }
        : null
  )
</script>

{#snippet gridButton(label: string, title: string, onclick: (e: MouseEvent) => void)}
  <button
    class="tabular h-6 min-w-7 rounded border border-line-soft px-1.5 font-mono text-[11px] font-semibold text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
    disabled={player.editing}
    aria-label={title}
    {title}
    {onclick}
  >
    {label}
  </button>
{/snippet}

{#snippet iconButton(
  Icon: Component<{ class?: string }>,
  title: string,
  onclick: () => void,
  disabled = false,
  flagged = false
)}
  <button
    class={[
      'grid size-6 place-items-center rounded border disabled:opacity-35',
      flagged ? 'border-flag/35 text-flag hover:bg-flag/15' : 'border-line-soft text-muted hover:bg-hover hover:text-fg'
    ]}
    disabled={player.editing || disabled}
    aria-label={title}
    {title}
    {onclick}
  >
    <Icon class="size-3.5" />
  </button>
{/snippet}

<div class="flex h-14 w-[344px] shrink-0 flex-col justify-between" role="toolbar" aria-label="Beatgrid editor">
  <div class="flex items-center gap-1">
    <span class="mr-auto flex items-baseline gap-1">
      <BpmField bpm={sectionBpm} />
      <span class="text-[11px] font-medium text-faint">BPM</span>
      {#if sectionChip}
        <span
          class="rounded-[3px] bg-flag/18 px-1 text-[9.5px] font-bold tracking-wider text-flag"
          title={sectionChip.title}>{sectionChip.label}</span
        >
      {/if}
    </span>
    {@render gridButton('←', 'Shift this section left by 1 ms (Shift: 10 ms)', (e) =>
      grid.nudge(e.shiftKey ? -10 : -1)
    )}
    {@render gridButton('→', 'Shift this section right by 1 ms (Shift: 10 ms)', (e) => grid.nudge(e.shiftKey ? 10 : 1))}
    {@render gridButton('/2', "Halve this section's BPM", () => grid.halve())}
    {@render gridButton('×2', "Double this section's BPM", () => grid.double())}
  </div>
  <div class="flex items-center gap-1">
    {@render gridButton('Set', 'Move the nearest beat to the playhead and make it the downbeat', () =>
      grid.setDownbeatHere()
    )}
    <button
      class={[
        'grid size-6 place-items-center rounded border transition',
        grid.link
          ? 'border-transparent bg-accent-soft text-accent'
          : 'border-line-soft text-faint hover:bg-hover hover:text-muted'
      ]}
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
    {@render iconButton(
      FlagTriangleRight,
      'Add a tempo flag on the beat nearest the playhead (T)',
      () => grid.addFlagAtPlayhead(),
      false,
      true
    )}
    {@render iconButton(ChevronLeft, 'Previous tempo flag (,)', () => grid.jumpToFlag(-1))}
    {@render iconButton(ChevronRight, 'Next tempo flag (.)', () => grid.jumpToFlag(1))}
    <span class="mx-0.5 h-4 w-px bg-line-soft"></span>
    {@render iconButton(Undo2, 'Undo (⌘Z)', () => grid.undo(), !grid.canUndo)}
    {@render iconButton(Redo2, 'Redo (⇧⌘Z)', () => grid.redo(), !grid.canRedo)}
    <button
      class="ml-auto h-6 rounded px-2 text-[11px] font-semibold text-muted hover:bg-hover hover:text-fg disabled:opacity-40"
      disabled={player.editing}
      onclick={() => grid.cancel()}
      title="Discard grid changes (Esc)"
    >
      Cancel
    </button>
    <button
      class="flex h-6 items-center gap-1 rounded bg-accent px-2.5 text-[11px] font-semibold text-bg hover:brightness-110 disabled:opacity-40"
      disabled={player.editing}
      onclick={onsave}
      title={grid.dirty ? 'Save the beatgrid to rekordbox' : 'No changes to save'}
    >
      {#if player.editing}<LoaderCircle class="size-3 animate-spin" />{/if}
      Save
    </button>
  </div>
</div>
