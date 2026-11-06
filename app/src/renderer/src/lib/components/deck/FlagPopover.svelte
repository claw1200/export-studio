<script lang="ts">
  import { tick } from 'svelte'
  import { X } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { formatBeatLength, formatClock } from '$lib/format'
  import { MAX_GRID_BPM, MIN_GRID_BPM, MIN_RAMP_BEATS, varies } from '$lib/tempo-map'

  const { xOf, width }: { xOf: (ms: number) => number; width: number } = $props()

  const { player } = getApp()
  const grid = player.grid
  const POPOVER_WIDTH = 248
  const EDGE_MARGIN = 8
  const DEFAULT_RAMP_BEATS = 16
  const RAMP_LENGTHS = [1, 2, 4, 8, 16, 32].map((bars) => bars * 4)
  const TRANSITIONS = [
    { label: 'Jump', ramps: false },
    { label: 'Ramp', ramps: true }
  ]

  const index = $derived(grid.map?.flags.findIndex((flag) => flag.id === grid.selectedFlagId) ?? -1)
  const flag = $derived(index >= 0 ? grid.map?.flags[index] : undefined)
  const title = $derived(index === 0 ? 'Grid start' : `Flag ${index + 1}`)
  const flagX = $derived(flag ? xOf(flag.startMs) : 0)
  const left = $derived(Math.max(EDGE_MARGIN, Math.min(width - POPOVER_WIDTH - EDGE_MARGIN, flagX - 22)))
  const arrowLeft = $derived(Math.max(10, Math.min(POPOVER_WIDTH - 20, flagX - left - 5)))
  const rampLengths = $derived(
    flag?.rampBeats && !RAMP_LENGTHS.includes(flag.rampBeats)
      ? [...RAMP_LENGTHS, flag.rampBeats].sort((a, b) => a - b)
      : RAMP_LENGTHS
  )

  let input: HTMLInputElement | undefined = $state()
  let text = $state('')
  let error = $state('')

  $effect(() => {
    if (!flag || document.activeElement === input) return
    text = flag.bpm.toFixed(2)
    error = ''
  })

  $effect(() => {
    if (!flag || grid.justAddedFlagId !== flag.id) return
    grid.justAddedFlagId = null
    void tick().then(() => input?.select())
  })

  function commit(): void {
    const bpm = Number(text.trim().replace(',', '.'))
    const valid = bpm >= MIN_GRID_BPM && bpm <= MAX_GRID_BPM
    error = valid ? '' : `Enter a tempo from ${MIN_GRID_BPM} to ${MAX_GRID_BPM} BPM.`
    if (valid) grid.setFlagBpm(index, bpm)
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    const inside = e.target instanceof Element && e.target.closest('[data-flag-popover], [data-tempo-lane]')
    if (flag && !inside) grid.select(null)
  }}
/>

{#snippet note(text: string)}
  <p class="text-[11px] leading-snug text-faint">{text}</p>
{/snippet}

{#if flag}
  <div
    data-flag-popover
    class="absolute top-[calc(100%+7px)] z-40 flex flex-col gap-2.5 rounded-lg border border-line bg-raised p-2.5 shadow-xl shadow-black/50"
    style:left="{left}px"
    style:width="{POPOVER_WIDTH}px"
    role="dialog"
    aria-label={title}
  >
    <span
      class="absolute -top-[6px] size-2.5 rotate-45 border-t border-l border-line bg-raised"
      style:left="{arrowLeft}px"
    ></span>
    <div class="flex items-center gap-2">
      <span class="size-2 rounded-[2px] bg-flag"></span>
      <span class="text-[12px] font-semibold text-fg">{title}</span>
      <span class="tabular font-mono text-[11px] text-faint">{formatClock(flag.startMs)}</span>
      <button
        class="ml-auto grid size-5 place-items-center rounded text-faint hover:bg-hover hover:text-fg"
        onclick={() => grid.select(null)}
        aria-label="Close"
      >
        <X class="size-3.5" />
      </button>
    </div>

    <label class="flex items-baseline gap-1.5">
      <input
        bind:this={input}
        bind:value={text}
        class="tabular w-24 rounded border border-line bg-bg px-1.5 py-0.5 font-mono text-[15px] font-semibold text-fg focus:border-accent/60"
        inputmode="decimal"
        aria-label="Flag tempo in BPM"
        disabled={player.editing}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            commit()
            if (!error) input?.blur()
          } else if (e.key === 'Escape') {
            e.preventDefault()
            e.stopPropagation()
            text = flag.bpm.toFixed(2)
            input?.blur()
          }
        }}
        onblur={commit}
      />
      <span class="text-[11px] font-medium text-faint">BPM</span>
    </label>
    {#if error}<p class="-mt-1 text-[11px] text-danger">{error}</p>{/if}

    {#if varies(flag)}
      {@render note(
        'The tempo here varies beat by beat, as rekordbox analysed it. Setting a tempo replaces it with a steady one.'
      )}
    {/if}

    {#if index === 0}
      {@render note('The first flag sets where the grid starts and its tempo.')}
    {:else}
      <div
        class="grid grid-cols-2 gap-0.5 rounded-md border border-line bg-bg p-0.5"
        role="group"
        aria-label="Transition"
      >
        {#each TRANSITIONS as { label, ramps } (label)}
          {@const on = ramps === flag.rampBeats > 0}
          <button
            class={[
              'rounded py-0.5 text-[11.5px] font-semibold',
              on ? 'bg-flag/20 text-flag' : 'text-faint hover:text-muted'
            ]}
            aria-pressed={on}
            disabled={player.editing}
            onclick={() => grid.setRamp(index, ramps ? flag.rampBeats || DEFAULT_RAMP_BEATS : 0)}
          >
            {label}
          </button>
        {/each}
      </div>
      {#if flag.rampBeats > 0}
        <label class="flex items-center gap-2 text-[11.5px] text-muted">
          Ramp over
          <select
            class="flex-1 rounded border border-line bg-bg px-1 py-0.5 text-[11.5px] text-fg"
            value={flag.rampBeats}
            disabled={player.editing}
            onchange={(e) => grid.setRamp(index, Math.max(MIN_RAMP_BEATS, Number(e.currentTarget.value)))}
          >
            {#each rampLengths as beats (beats)}
              <option value={beats}>{formatBeatLength(beats)}</option>
            {/each}
          </select>
        </label>
      {/if}
      <label class="flex items-center gap-2 text-[11.5px] text-muted">
        <input
          type="checkbox"
          class="accent-flag"
          checked={flag.beatNumber === 1}
          disabled={player.editing}
          onchange={(e) => grid.setBarStart(index, e.currentTarget.checked)}
        />
        Bar starts here
      </label>
      <div class="flex border-t border-line-soft pt-2.5">
        <button
          class="ml-auto h-6 rounded border border-line bg-surface px-2 text-[11px] font-semibold text-danger hover:bg-hover"
          disabled={player.editing}
          onclick={() => grid.removeFlag(index)}
        >
          Delete flag
        </button>
      </div>
    {/if}
  </div>
{/if}
