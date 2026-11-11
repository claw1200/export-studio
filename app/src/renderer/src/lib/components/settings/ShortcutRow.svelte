<script lang="ts">
  import { Plus, RotateCcw, X } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { ACTIONS, chordKeys, chordLabel, type ActionId } from '$lib/keymap'

  let { id, recording, ontogglerecording }: { id: ActionId; recording: boolean; ontogglerecording: () => void } =
    $props()

  const { keybinds } = getApp()

  const spec = $derived(ACTIONS[id])
  const chords = $derived(keybinds.bindings[id])
</script>

<div
  class={[
    'group/row flex min-h-9 items-center gap-3 rounded-md px-2 py-1',
    recording ? 'bg-accent-soft' : 'hover:bg-hover/60'
  ]}
>
  <span class="min-w-0 flex-1 truncate text-[12.5px]">
    {spec.label}
    {#if 'hold' in spec && spec.hold}
      <span class="ml-1 text-[10.5px] text-faint">hold</span>
    {/if}
  </span>
  <div class="flex flex-wrap items-center justify-end gap-1">
    {#each chords as chord, i (chord)}
      {#if i > 0}
        <span class="px-0.5 text-[11px] text-faint">or</span>
      {/if}
      <span class="group/chip flex items-center gap-1 rounded-md py-0.5 pl-1 hover:bg-hover">
        {#each chordKeys(chord) as key, k (k)}
          {#if k > 0}
            <span class="text-[11px] text-faint" aria-hidden="true">+</span>
          {/if}
          <kbd
            class="grid h-6 min-w-6 place-items-center rounded-md border border-b-2 border-line bg-bg px-1.5 font-sans text-[12px] font-medium text-fg"
          >
            {key}
          </kbd>
        {/each}
        <button
          class="invisible grid size-5 place-items-center rounded text-faint group-hover/chip:visible hover:bg-raised hover:text-fg focus-visible:visible"
          onclick={() => keybinds.removeChord(id, chord)}
          aria-label="Remove {chordLabel(chord)} from {spec.label}"
          title="Remove"
        >
          <X class="size-3" />
        </button>
      </span>
    {/each}
    {#if recording}
      <span class="flex h-7 animate-pulse items-center rounded-md border border-accent/70 px-2 text-[12px] text-accent">
        Press keys…
      </span>
    {:else if !chords.length}
      <span class="px-1 text-[12px] text-faint">Not set</span>
    {/if}
  </div>
  <div class="flex w-14 shrink-0 justify-end gap-0.5">
    <button
      class={[
        'grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg',
        recording && 'bg-accent/20 text-accent'
      ]}
      onclick={ontogglerecording}
      aria-label={recording ? 'Cancel adding a shortcut' : `Add a shortcut for ${spec.label}`}
      title={recording ? 'Cancel' : 'Add shortcut'}
    >
      <Plus class="size-3.5" />
    </button>
    <button
      class="grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg disabled:invisible"
      disabled={!keybinds.isCustomised(id)}
      onclick={() => keybinds.resetBinding(id)}
      aria-label="Reset {spec.label} to default"
      title="Reset to default"
    >
      <RotateCcw class="size-3" />
    </button>
  </div>
</div>
