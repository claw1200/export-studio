<script lang="ts">
  import { Link, LoaderCircle, Unlink } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import BpmField from '../BpmField.svelte'

  const { bpm, onsave }: { bpm: number; onsave: () => void } = $props()

  const { player } = getApp()
  const grid = player.grid
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

<div class="flex h-14 w-[328px] shrink-0 flex-col justify-between" role="toolbar" aria-label="Beatgrid editor">
  <div class="flex items-center gap-1">
    <span class="mr-auto flex items-baseline gap-1">
      <BpmField {bpm} />
      <span class="text-[11px] font-medium text-faint">BPM</span>
    </span>
    {@render gridButton('←', 'Shift the beatgrid left by 1 ms (Shift: 10 ms)', (e) =>
      grid.nudge(e.shiftKey ? -10 : -1)
    )}
    {@render gridButton('→', 'Shift the beatgrid right by 1 ms (Shift: 10 ms)', (e) => grid.nudge(e.shiftKey ? 10 : 1))}
    {@render gridButton('/2', 'Halve the BPM', () => grid.halve())}
    {@render gridButton('×2', 'Double the BPM', () => grid.double())}
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
