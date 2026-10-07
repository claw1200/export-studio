<script lang="ts">
  import { Keyboard, Plus, RotateCcw, Search, SlidersHorizontal, Wrench, X } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { ACTION_IDS, ACTIONS, chordFromEvent, chordLabel, type ActionGroup, type ActionId } from '$lib/keymap'
  import type { CuePlacement } from '$lib/player.svelte'

  let { onclose }: { onclose: () => void } = $props()

  const { library, player, keybinds } = getApp()

  type Section = 'general' | 'keyboard' | 'developer'
  const SECTIONS: { id: Section; label: string; icon: typeof Keyboard }[] = [
    { id: 'general', label: 'General', icon: SlidersHorizontal },
    { id: 'keyboard', label: 'Keyboard shortcuts', icon: Keyboard },
    { id: 'developer', label: 'Developer options', icon: Wrench }
  ]
  const GROUPS: ActionGroup[] = ['Playback', 'Cues', 'Library', 'App']
  const PLACEMENTS: { id: CuePlacement; label: string }[] = [
    { id: 'playhead', label: 'Playhead' },
    { id: 'cuePoint', label: 'Cue point' }
  ]

  let section = $state<Section>('general')
  let recording = $state<ActionId | null>(null)
  let notice = $state<string | null>(null)
  let filter = $state('')
  let swallowKeyup: string | null = null

  const groups = $derived.by(() => {
    const q = filter.trim().toLowerCase()
    const matches = (id: ActionId): boolean =>
      !q ||
      ACTIONS[id].label.toLowerCase().includes(q) ||
      keybinds.bindings[id].some((chord) => chordLabel(chord).toLowerCase().includes(q))
    return GROUPS.map((group) => ({
      group,
      ids: ACTION_IDS.filter((id) => ACTIONS[id].group === group && matches(id))
    })).filter((g) => g.ids.length)
  })

  function modal(dialog: HTMLDialogElement): () => void {
    const resume = keybinds.suspend()
    dialog.showModal()
    return resume
  }

  function startRecording(id: ActionId): void {
    notice = null
    recording = recording === id ? null : id
  }

  function onkeydown(e: KeyboardEvent): void {
    if (!recording) return
    e.preventDefault()
    e.stopPropagation()
    if (e.key === 'Escape') {
      recording = null
      return
    }
    const chord = chordFromEvent(e)
    if (!chord) return
    const id = recording
    recording = null
    swallowKeyup = e.code
    const owner = keybinds.addChord(id, chord)
    notice = owner ? `${chordLabel(chord)} was moved from “${ACTIONS[owner].label}” to “${ACTIONS[id].label}”.` : null
  }

  function onkeyup(e: KeyboardEvent): void {
    if (swallowKeyup !== e.code) return
    swallowKeyup = null
    e.preventDefault()
  }

  function resetAll(): void {
    recording = null
    notice = null
    keybinds.resetAll()
  }
</script>

<dialog
  {@attach modal}
  class="no-drag m-auto h-[600px] max-h-[calc(100vh-48px)] w-[780px] max-w-[calc(100vw-48px)] overflow-hidden rounded-xl border border-line bg-surface p-0 text-fg shadow-2xl shadow-black/60 backdrop:bg-black/60"
  aria-label="Settings"
  {onkeydown}
  {onkeyup}
  oncancel={(e) => {
    if (!recording) return
    e.preventDefault()
    recording = null
  }}
  onclose={() => onclose()}
  onclick={(e) => {
    if (e.target === e.currentTarget) e.currentTarget.close()
  }}
>
  <div class="flex h-full">
    <nav class="flex w-48 shrink-0 flex-col gap-0.5 border-r border-line-soft bg-bg/50 p-3">
      <div class="px-2 pt-1 pb-3 text-[13px] font-semibold">Settings</div>
      {#each SECTIONS as s (s.id)}
        <button
          class={[
            'flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12.5px]',
            section === s.id ? 'bg-raised text-fg' : 'text-muted hover:bg-hover hover:text-fg'
          ]}
          aria-current={section === s.id ? 'page' : undefined}
          onclick={() => {
            section = s.id
            recording = null
          }}
        >
          <s.icon class="size-3.5" />
          {s.label}
        </button>
      {/each}
    </nav>

    <div class="flex min-w-0 flex-1 flex-col">
      <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft px-5">
        <h2 class="text-[13px] font-semibold">{SECTIONS.find((s) => s.id === section)?.label}</h2>
        {#if section === 'keyboard'}
          <div class="relative ml-auto w-56">
            <Search class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-faint" />
            <input
              bind:value={filter}
              type="search"
              placeholder="Filter shortcuts"
              spellcheck="false"
              class="h-7 w-full rounded-md border border-line bg-bg pr-2 pl-7 text-[12px] text-fg placeholder:text-faint focus:border-accent/60 focus:outline-none [&::-webkit-search-cancel-button]:hidden"
            />
          </div>
        {/if}
        <button
          class={[
            'grid size-7 place-items-center rounded-md text-faint hover:bg-hover hover:text-fg',
            section !== 'keyboard' && 'ml-auto'
          ]}
          onclick={(e) => e.currentTarget.closest('dialog')?.close()}
          aria-label="Close settings"
        >
          <X class="size-4" />
        </button>
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {#if section === 'developer'}
          <div class="flex items-center gap-3 rounded-md px-2 py-2">
            <span class="min-w-0 flex-1">
              <span class="block text-[12.5px] font-medium">Fall blocks</span>
              <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                How many blocks a 3-band fall slopes across. 0 keeps the steps.
              </span>
            </span>
            <input
              class="h-4 w-28 shrink-0 cursor-pointer accent-accent"
              type="range"
              min="0"
              max="100"
              step="1"
              aria-label="Fall blocks"
              aria-valuetext={String(player.fallBlocks)}
              value={player.fallBlocks}
              oninput={(e) => player.setFallBlocks(Number(e.currentTarget.value))}
            />
            <span class="w-9 shrink-0 text-right font-mono text-[11px] text-muted tabular-nums">
              {player.fallBlocks}
            </span>
          </div>
        {:else if section === 'general'}
          <div class="flex flex-col gap-1">
            <div class="flex items-center gap-3 rounded-md px-2 py-2">
              <span class="min-w-0 flex-1">
                <span class="block text-[12.5px] font-medium">High quality waveforms</span>
                <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                  Draw the detail waveform from the audio. Off uses the rekordbox analysis. Higher keeps more detail.
                </span>
              </span>
              <input
                class="h-4 w-28 shrink-0 cursor-pointer accent-accent"
                type="range"
                min="0"
                max="100"
                step="1"
                aria-label="High quality waveforms"
                aria-valuetext={player.waveQuality === 0 ? 'Off' : `${player.waveQuality}%`}
                value={player.waveQuality}
                oninput={(e) => player.setWaveQuality(Number(e.currentTarget.value))}
              />
              <span class="w-9 shrink-0 text-right font-mono text-[11px] text-muted tabular-nums">
                {player.waveQuality === 0 ? 'Off' : `${player.waveQuality}%`}
              </span>
            </div>
            {@render toggle(
              'Quantise',
              'Scrubbing, the cue point and new cues snap to the nearest beat.',
              player.quantise,
              () => player.setQuantise(!player.quantise)
            )}
            <div class="flex items-start gap-3 rounded-md px-2 py-2">
              <span class="min-w-0 flex-1">
                <span class="block text-[12.5px] font-medium">Place new cues at</span>
                <span class="mt-0.5 block text-[11px] leading-snug text-faint">
                  Where new hot cues and memory cues go, from the buttons or the keyboard.
                </span>
              </span>
              <div
                class="flex shrink-0 rounded-md border border-line bg-bg p-0.5 text-[11px] font-semibold"
                role="radiogroup"
                aria-label="Place new cues at"
              >
                {#each PLACEMENTS as p (p.id)}
                  <button
                    class={[
                      'rounded px-2.5 py-1',
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
            {@render toggle(
              'Write track info to file tags',
              'When you edit title, artist, album and other text fields, also save them into the audio files’ own tags. Off keeps changes in rekordbox only. Rating and colour always stay in rekordbox.',
              library.writeFileTags,
              () => library.setWriteFileTags(!library.writeFileTags)
            )}
          </div>
        {:else}
          {#each groups as { group, ids } (group)}
            <section class="mb-5 last:mb-0">
              <h3 class="mb-1 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">{group}</h3>
              {#each ids as id (id)}
                {@render shortcutRow(id)}
              {/each}
            </section>
          {:else}
            <p class="px-2 text-[12px] text-faint">No shortcuts match “{filter}”.</p>
          {/each}
        {/if}
      </div>

      {#if section === 'keyboard'}
        <footer class="flex h-11 shrink-0 items-center gap-3 border-t border-line-soft px-5 text-[11.5px]">
          {#if recording}
            <span class="text-accent">Press a key combination. Esc cancels.</span>
          {:else if notice}
            <span class="truncate text-warn" title={notice}>{notice}</span>
          {:else}
            <span class="text-faint">Click + to add a shortcut. Hover a shortcut to remove it.</span>
          {/if}
          <button
            class="ml-auto flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-muted hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-40"
            disabled={!keybinds.customised}
            onclick={resetAll}
          >
            <RotateCcw class="size-3" />
            Reset all to defaults
          </button>
        </footer>
      {/if}
    </div>
  </div>
</dialog>

{#snippet toggle(label: string, description: string, on: boolean, flip: () => void)}
  <button
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

{#snippet shortcutRow(id: ActionId)}
  {@const spec = ACTIONS[id]}
  {@const chords = keybinds.bindings[id]}
  <div
    class={[
      'group/row flex min-h-9 items-center gap-3 rounded-md px-2 py-1',
      recording === id ? 'bg-accent-soft' : 'hover:bg-hover/60'
    ]}
  >
    <span class="min-w-0 flex-1 truncate text-[12.5px]">
      {spec.label}
      {#if 'hold' in spec && spec.hold}
        <span class="ml-1 text-[10.5px] text-faint">hold</span>
      {/if}
    </span>
    <div class="flex flex-wrap items-center justify-end gap-1">
      {#each chords as chord (chord)}
        <span
          class="group/chip relative flex h-6 items-center rounded border border-line bg-bg px-1.5 font-mono text-[11px] text-fg"
        >
          {chordLabel(chord)}
          <button
            class="ml-1 hidden size-4 place-items-center rounded text-faint group-hover/chip:grid hover:bg-hover hover:text-fg focus-visible:grid"
            onclick={() => keybinds.removeChord(id, chord)}
            aria-label="Remove {chordLabel(chord)} from {spec.label}"
            title="Remove"
          >
            <X class="size-3" />
          </button>
        </span>
      {/each}
      {#if recording === id}
        <span
          class="flex h-6 animate-pulse items-center rounded border border-accent/70 px-1.5 text-[11px] text-accent"
        >
          Press keys…
        </span>
      {:else if !chords.length}
        <span class="px-1 text-[11px] text-faint">Not set</span>
      {/if}
    </div>
    <div class="flex w-14 shrink-0 justify-end gap-0.5">
      <button
        class={[
          'grid size-6 place-items-center rounded text-faint hover:bg-hover hover:text-fg',
          recording === id && 'bg-accent/20 text-accent'
        ]}
        onclick={() => startRecording(id)}
        aria-label={recording === id ? 'Cancel adding a shortcut' : `Add a shortcut for ${spec.label}`}
        title={recording === id ? 'Cancel' : 'Add shortcut'}
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
{/snippet}
