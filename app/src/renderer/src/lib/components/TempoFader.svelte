<script lang="ts">
  import { RotateCcw } from '@lucide/svelte'
  import type { Attachment } from 'svelte/attachments'
  import { getApp } from '$lib/context'
  import { formatTempo } from '$lib/deck/tempo.svelte'

  const { player } = getApp()
  const tempo = player.tempo

  /** Shift moves the fader this much finer, by drag, wheel or key. */
  const FINE = 0.1
  /** Percent per arrow key or wheel notch. */
  const STEP = 0.05
  const PAGE = 1

  let track: HTMLDivElement | undefined
  /** Where the drag has taken the tempo, before rounding, so slow moves still add up. */
  let drag: { y: number; percent: number } | null = null

  /** −1 at the top (slowest) to +1 at the bottom (fastest): faster is towards you, as on a CDJ. */
  const travel = $derived(Math.max(-1, Math.min(1, tempo.percent / tempo.range)))
  const atZero = $derived(Math.abs(tempo.percent) < 0.005)

  function percentAt(clientY: number): number {
    if (!track) return tempo.percent
    const rect = track.getBoundingClientRect()
    return ((clientY - rect.top) / rect.height) * 2 * tempo.range - tempo.range
  }

  function pointerdown(e: PointerEvent & { currentTarget: HTMLDivElement }): void {
    if (e.button !== 0) return
    e.currentTarget.setPointerCapture(e.pointerId)
    e.currentTarget.focus()
    const onThumb = e.target instanceof Element && e.target.closest('[data-thumb]')
    if (!onThumb) tempo.setPercent(percentAt(e.clientY))
    drag = { y: e.clientY, percent: tempo.percent }
  }

  function pointermove(e: PointerEvent): void {
    if (!drag || !track) return
    const perPixel = ((tempo.range * 2) / track.clientHeight) * (e.shiftKey ? FINE : 1)
    drag = { y: e.clientY, percent: drag.percent + (e.clientY - drag.y) * perPixel }
    tempo.setPercent(drag.percent)
  }

  function keydown(e: KeyboardEvent): void {
    const step = e.shiftKey ? STEP * FINE : STEP
    const moves: Record<string, number> = { ArrowDown: step, ArrowUp: -step, PageDown: PAGE, PageUp: -PAGE }
    const move = moves[e.key]
    if (move == null) return
    e.preventDefault()
    tempo.setPercent(tempo.percent + move)
  }

  /** Keeps the track for measuring drags, and listens to the wheel in a way that can stop the
   * page scrolling, which Svelte's own `onwheel` cannot. */
  const fader: Attachment<HTMLDivElement> = (node) => {
    track = node
    const onwheel = (e: WheelEvent): void => {
      if (e.deltaY === 0) return
      e.preventDefault()
      const step = e.shiftKey ? STEP * FINE : STEP
      tempo.setPercent(tempo.percent + Math.sign(e.deltaY) * step)
    }
    node.addEventListener('wheel', onwheel, { passive: false })
    return () => {
      node.removeEventListener('wheel', onwheel)
      track = undefined
    }
  }

  const smallButton =
    'flex h-5 items-center justify-center rounded border text-[10px] font-bold tracking-wide transition disabled:opacity-30'
</script>

<div class="flex w-[132px] shrink-0 gap-3 border-l border-line-soft px-3 py-3">
  <div class="flex flex-col items-center text-[10px] leading-none font-semibold text-faint">
    <span aria-hidden="true">−</span>
    <div
      class="relative my-1 w-5 flex-1 cursor-ns-resize touch-none rounded outline-none focus-visible:ring-1 focus-visible:ring-accent"
      role="slider"
      tabindex="0"
      aria-label="Tempo"
      aria-orientation="vertical"
      aria-valuemin={-tempo.range}
      aria-valuemax={tempo.range}
      aria-valuenow={Number(tempo.percent.toFixed(2))}
      aria-valuetext={formatTempo(tempo.percent)}
      title="Tempo: drag, scroll or use the arrow keys. Shift for fine steps, double-click to reset."
      onpointerdown={pointerdown}
      onpointermove={pointermove}
      onpointerup={() => (drag = null)}
      onpointercancel={() => (drag = null)}
      onlostpointercapture={() => (drag = null)}
      ondblclick={() => tempo.reset()}
      onkeydown={keydown}
      {@attach fader}
    >
      <div class="absolute inset-y-0 left-1/2 w-1 -translate-x-1/2 rounded-full bg-line"></div>
      <div class="absolute inset-x-0.5 top-1/2 h-px bg-faint/70"></div>
      <div
        data-thumb
        class="absolute inset-x-0 h-3 -translate-y-1/2 rounded-sm border border-line bg-raised shadow-md shadow-black/50"
        style:top="{50 + travel * 50}%"
      >
        <div class={['absolute inset-x-1 top-1/2 h-px', atZero ? 'bg-accent' : 'bg-fg/80']}></div>
      </div>
    </div>
    <span aria-hidden="true">+</span>
  </div>

  <div class="flex min-w-0 flex-1 flex-col justify-between">
    <div>
      <div class="text-[9.5px] font-semibold tracking-wider text-faint">TEMPO</div>
      <div class={['tabular font-mono text-[14px] font-medium', atZero ? 'text-muted' : 'text-fg']}>
        {formatTempo(tempo.percent)}
      </div>
    </div>
    <div class="grid grid-cols-2 gap-1">
      <button
        class={[smallButton, 'border-line-soft text-muted hover:bg-hover hover:text-fg']}
        onclick={() => tempo.cycleRange()}
        aria-label="Tempo range ±{tempo.range}%"
        title="Tempo range: ±{tempo.range}%. Click for the next range."
      >
        ±{tempo.range}
      </button>
      <button
        class={[
          smallButton,
          tempo.masterTempo
            ? 'border-transparent bg-accent-soft text-accent'
            : 'border-line-soft text-faint hover:bg-hover hover:text-muted'
        ]}
        onclick={() => tempo.setMasterTempo(!tempo.masterTempo)}
        aria-pressed={tempo.masterTempo}
        aria-label="Master tempo"
        title={tempo.masterTempo
          ? 'Master tempo on: the key stays the same as the tempo changes'
          : 'Master tempo off: the pitch moves with the tempo'}
      >
        MT
      </button>
      <button
        class={[smallButton, 'col-span-2 gap-1 border-line-soft text-faint hover:bg-hover hover:text-muted']}
        disabled={atZero}
        onclick={() => tempo.reset()}
        title="Back to the track's own tempo"
      >
        <RotateCcw class="size-2.5" />
        RESET
      </button>
    </div>
  </div>
</div>
