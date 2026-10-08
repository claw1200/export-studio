<script lang="ts">
  import { Pencil, Plus, X } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import {
    cueColor,
    formatClock,
    HOT_CUE_KINDS,
    HOT_CUE_LETTERS,
    HOT_CUE_PICKER,
    hotCueLetter,
    hotCuePaletteColor
  } from '$lib/format'
  import type { Cue } from '$lib/types'

  const { library, player } = getApp()

  const hotCues = $derived.by(() => {
    const bySlot = new Map(
      (player.detail?.cues ?? []).filter((c) => c.kind === 'hot').map((c) => [hotCueLetter(c.slot), c])
    )
    return [...HOT_CUE_LETTERS].map((letter, i) => ({
      letter,
      kind: HOT_CUE_KINDS[i],
      cue: bySlot.get(letter) ?? null
    }))
  })
  const memoryCount = $derived((player.detail?.cues ?? []).filter((c) => c.kind === 'memory').length)
  const editable = $derived(library.writable && !!player.detail && !player.grid.editing)
  const placementLabel = $derived(player.cuePlacement === 'cuePoint' ? 'cue point' : 'playhead')
  /** Closes whenever editing turns on or off. */
  let pickerCueId = $derived.by((): string | null => {
    void editable
    return null
  })

  function pickColor(cue: Cue, colorIndex: number | null): void {
    pickerCueId = null
    void player.cues.setCueColor(cue, colorIndex)
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== 'Escape' || e.defaultPrevented || !pickerCueId) return
    e.preventDefault()
    pickerCueId = null
  }}
/>

{#each hotCues as { letter, kind, cue } (letter)}
  {#if cue}
    <div class="group relative">
      <button
        class="flex h-7 w-[72px] touch-none items-center gap-1.5 rounded-md px-2 text-[11px] font-semibold text-bg transition select-none hover:brightness-110"
        style:background={cueColor(cue)}
        title={cue.comment || undefined}
        onpointerdown={(e) => {
          if (e.button !== 0) return
          e.currentTarget.setPointerCapture(e.pointerId)
          player.hotCueDown(kind)
        }}
        onpointerup={() => player.hotCueUp(kind)}
        onpointercancel={() => player.hotCueUp(kind)}
        onlostpointercapture={() => player.hotCueUp(kind)}
      >
        {letter}
        <span
          class={[
            'tabular truncate font-mono text-[10px] font-medium opacity-75 transition-opacity',
            editable && ['group-hover:opacity-0', pickerCueId === cue.id && 'opacity-0']
          ]}
        >
          {formatClock(cue.start).slice(0, 5)}
        </span>
      </button>
      {#if editable}
        <div
          class={[
            'absolute top-1/2 right-1 flex -translate-y-1/2 opacity-0 group-hover:opacity-100 focus-within:opacity-100',
            pickerCueId === cue.id && 'opacity-100'
          ]}
        >
          <button
            class="grid size-5 place-items-center rounded text-bg hover:bg-black/25 disabled:hidden"
            disabled={player.editing}
            onclick={() => (pickerCueId = pickerCueId === cue.id ? null : cue.id)}
            aria-label="Change colour of hot cue {letter}"
            aria-expanded={pickerCueId === cue.id}
            aria-haspopup="dialog"
            title="Change colour"
          >
            <Pencil class="size-3" />
          </button>
          <button
            class="grid size-5 place-items-center rounded text-bg hover:bg-black/25 disabled:hidden"
            disabled={player.editing}
            onclick={() => player.cues.deleteCue(cue)}
            aria-label="Delete hot cue {letter}"
            title="Delete hot cue {letter}"
          >
            <X class="size-3" />
          </button>
        </div>
      {/if}
      {#if editable && pickerCueId === cue.id}
        <button
          class="fixed inset-0 z-40 cursor-default"
          aria-label="Close colour picker"
          onclick={() => (pickerCueId = null)}
        ></button>
        <div
          class="absolute top-full left-0 z-50 mt-1.5 w-max rounded-lg border border-line bg-surface p-2 shadow-xl shadow-black/40"
          role="dialog"
          aria-label="Hot cue {letter} colour"
        >
          <div class="grid grid-cols-4 gap-1.5">
            {#each HOT_CUE_PICKER as index (index)}
              <button
                class={[
                  'size-6 rounded transition hover:scale-110',
                  cue.colorIndex === index && 'ring-2 ring-fg ring-offset-1 ring-offset-surface'
                ]}
                style:background={hotCuePaletteColor(index)}
                onclick={() => pickColor(cue, index)}
                aria-label="Colour {index}"
                aria-pressed={cue.colorIndex === index}
              ></button>
            {/each}
          </div>
          <button
            class="mt-2 w-full rounded-md px-2 py-1 text-left text-[11px] text-muted hover:bg-hover hover:text-fg disabled:opacity-50"
            disabled={!cue.colorIndex}
            onclick={() => pickColor(cue, null)}
          >
            No colour
          </button>
        </div>
      {/if}
    </div>
  {:else if editable}
    <button
      class="group flex h-7 w-[72px] items-center gap-1.5 rounded-md border border-dashed border-line px-2 text-[11px] font-semibold text-faint transition hover:border-accent/60 hover:bg-accent-soft hover:text-accent disabled:pointer-events-none disabled:opacity-50"
      disabled={player.editing}
      onclick={() => player.setHotCue(kind)}
      aria-label="Set hot cue {letter} at the {placementLabel}"
      title="Set hot cue {letter} at the {placementLabel}{player.quantise ? ', on the nearest beat' : ''}"
    >
      {letter}
      <Plus class="ml-auto size-3 opacity-0 group-hover:opacity-100" />
    </button>
  {:else}
    <div
      class="flex h-7 w-[72px] items-center rounded-md border border-line-soft px-2 text-[11px] font-semibold text-faint/60"
    >
      {letter}
    </div>
  {/if}
{/each}

<span class="ml-3 text-[11px] text-faint">{memoryCount} memory cues</span>
