<script lang="ts">
  import type { DeckLoop } from '$lib/deck/deck.svelte'
  import { cueColor, formatClock, hotCueLetter } from '$lib/format'
  import type { Cue } from '$lib/types'
  import { LOOP_ACTIVE_COLOR } from '$lib/waveform/colors'

  let {
    cues,
    loop = null,
    pct,
    draggable = undefined,
    onpointerdown,
    onpointerup,
    onpointercancel,
    onclick
  }: {
    cues: Cue[]
    loop?: DeckLoop | null
    pct: (seconds: number) => number
    draggable?: boolean
    onpointerdown?: (cue: Cue, e: PointerEvent) => void
    onpointerup?: (cue: Cue, e: PointerEvent) => void
    onpointercancel?: (cue: Cue, e: PointerEvent) => void
    onclick?: (cue: Cue, e: MouseEvent) => void
  } = $props()

  function label(cue: Cue): string {
    const name = cue.kind === 'hot' ? `Hot cue ${hotCueLetter(cue.slot)}` : 'Memory cue'
    const time = formatClock(cue.start)
    return cue.comment ? `${name}, ${time}, ${cue.comment}` : `${name}, ${time}`
  }
</script>

{#if loop}
  <div
    class={['pointer-events-none absolute inset-y-0 border-x', loop.active ? 'opacity-40' : 'opacity-15']}
    style:left="{pct(loop.start)}%"
    style:width="{pct(loop.end) - pct(loop.start)}%"
    style:background={LOOP_ACTIVE_COLOR}
    style:border-color={LOOP_ACTIVE_COLOR}
  ></div>
{/if}

{#each cues as cue (cue.id)}
  {#if cue.end != null}
    <div
      class="pointer-events-none absolute inset-y-0 opacity-25"
      style:left="{pct(cue.start / 1000)}%"
      style:width="{pct(cue.end / 1000) - pct(cue.start / 1000)}%"
      style:background={cueColor(cue)}
    ></div>
  {/if}
  <button
    type="button"
    class={[
      'absolute cursor-pointer border-0 bg-transparent p-0 hover:brightness-125',
      cue.kind === 'hot'
        ? 'top-0 z-20 grid size-3 place-items-center text-[8px] leading-none font-bold text-black/85'
        : 'top-0 z-10 flex h-4 w-4 -translate-x-1/2 items-start justify-center'
    ]}
    style:left="{pct(cue.start / 1000)}%"
    style:background={cue.kind === 'hot' ? cueColor(cue) : undefined}
    title={label(cue)}
    aria-label={label(cue)}
    draggable={draggable === false ? 'false' : undefined}
    onpointerdown={(e) => onpointerdown?.(cue, e)}
    onpointerup={(e) => onpointerup?.(cue, e)}
    onpointercancel={(e) => onpointercancel?.(cue, e)}
    onclick={(e) => onclick?.(cue, e)}
  >
    {#if cue.kind === 'hot'}
      {hotCueLetter(cue.slot)}
    {:else}
      <span
        class="pointer-events-none h-[7px] w-[9px]"
        style:background={cueColor(cue)}
        style:clip-path="polygon(0 0, 100% 0, 50% 100%)"
      ></span>
    {/if}
  </button>
{/each}
