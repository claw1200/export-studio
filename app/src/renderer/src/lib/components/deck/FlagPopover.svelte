<script lang="ts">
  import { tick } from 'svelte'
  import { X } from '@lucide/svelte'
  import { getApp } from '$lib/context'
  import { formatClock } from '$lib/format'
  import { MAX_GRID_BPM, MIN_GRID_BPM, MIN_RAMP, varies } from '$lib/tempo-map'

  /** Options for the open tempo flag, under its pennant in the tempo lane. */

  const { xOf, width }: { xOf: (ms: number) => number; width: number } = $props()

  const { player } = getApp()
  const grid = player.grid
  const POPOVER_W = 248
  const RAMP_BARS = [1, 2, 4, 8, 16, 32]

  const index = $derived(grid.map?.flags.findIndex((f) => f.id === grid.selected) ?? -1)
  const flag = $derived(index >= 0 ? grid.map?.flags[index] : undefined)
  const left = $derived(flag ? Math.max(8, Math.min(width - POPOVER_W - 8, xOf(flag.ms) - 22)) : 0)
  const arrow = $derived(flag ? Math.max(10, Math.min(POPOVER_W - 20, xOf(flag.ms) - left - 5)) : 0)
  const rampOptions = $derived.by(() => {
    const beats = RAMP_BARS.map((bars) => bars * 4)
    if (flag?.ramp && !beats.includes(flag.ramp)) beats.push(flag.ramp)
    return beats.sort((a, b) => a - b)
  })

  let input: HTMLInputElement | undefined = $state()
  let text = $state('')
  let error = $state('')

  $effect(() => {
    if (!flag || document.activeElement === input) return
    text = flag.bpm.toFixed(2)
    error = ''
  })

  // A flag just added opens ready to type its tempo.
  $effect(() => {
    if (!flag || grid.added !== flag.id) return
    grid.added = null
    void tick().then(() => {
      input?.focus()
      input?.select()
    })
  })

  function commit(): void {
    if (!flag) return
    const value = Number(text.trim().replace(',', '.'))
    if (!(value >= MIN_GRID_BPM && value <= MAX_GRID_BPM)) {
      error = `Enter a tempo from ${MIN_GRID_BPM} to ${MAX_GRID_BPM} BPM.`
      return
    }
    error = ''
    grid.setFlagBpm(index, value)
  }

  function barsLabel(beats: number): string {
    if (beats % 4) return `${beats} beats`
    return beats === 4 ? '1 bar' : `${beats / 4} bars`
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (!flag || !(e.target instanceof Element)) return
    if (e.target.closest('[data-flag-popover], [role="toolbar"][aria-label^="Tempo flags"]')) return
    grid.select(null)
  }}
/>

{#if flag}
  <div
    data-flag-popover
    class="absolute top-[calc(100%+7px)] z-40 flex flex-col gap-2.5 rounded-lg border border-line bg-raised p-2.5 shadow-xl shadow-black/50"
    style:left="{left}px"
    style:width="{POPOVER_W}px"
    role="dialog"
    aria-label={index === 0 ? 'Grid start' : `Tempo flag ${index + 1}`}
  >
    <span class="absolute -top-[6px] size-2.5 rotate-45 border-t border-l border-line bg-raised" style:left="{arrow}px"
    ></span>
    <div class="flex items-center gap-2">
      <span class="size-2 rounded-[2px] bg-[#b48cff]"></span>
      <span class="text-[12px] font-semibold text-fg">{index === 0 ? 'Grid start' : `Flag ${index + 1}`}</span>
      <span class="tabular font-mono text-[11px] text-faint">{formatClock(flag.ms)}</span>
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
      <p class="text-[11px] leading-snug text-faint">
        The tempo here varies beat by beat, as rekordbox analysed it. Setting a tempo replaces it with a steady one.
      </p>
    {/if}

    {#if index === 0}
      <p class="text-[11px] leading-snug text-faint">The first flag sets where the grid starts and its tempo.</p>
    {:else}
      <div
        class="grid grid-cols-2 gap-0.5 rounded-md border border-line bg-bg p-0.5"
        role="group"
        aria-label="Transition"
      >
        {#each [{ id: 'jump', label: 'Jump' }, { id: 'ramp', label: 'Ramp' }] as option (option.id)}
          {@const on = option.id === 'ramp' ? flag.ramp > 0 : flag.ramp === 0}
          <button
            class={[
              'rounded py-0.5 text-[11.5px] font-semibold',
              on ? 'bg-[#b48cff]/20 text-[#b48cff]' : 'text-faint hover:text-muted'
            ]}
            aria-pressed={on}
            disabled={player.editing}
            onclick={() => grid.setRamp(index, option.id === 'ramp' ? flag.ramp || 16 : 0)}
          >
            {option.label}
          </button>
        {/each}
      </div>
      {#if flag.ramp > 0}
        <label class="flex items-center gap-2 text-[11.5px] text-muted">
          Ramp over
          <select
            class="flex-1 rounded border border-line bg-bg px-1 py-0.5 text-[11.5px] text-fg"
            value={flag.ramp}
            disabled={player.editing}
            onchange={(e) => grid.setRamp(index, Math.max(MIN_RAMP, Number(e.currentTarget.value)))}
          >
            {#each rampOptions as beats (beats)}
              <option value={beats}>{barsLabel(beats)}</option>
            {/each}
          </select>
        </label>
      {/if}
      <label class="flex items-center gap-2 text-[11.5px] text-muted">
        <input
          type="checkbox"
          class="accent-[#b48cff]"
          checked={flag.beat === 1}
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
