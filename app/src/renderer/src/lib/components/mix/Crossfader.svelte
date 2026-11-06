<script lang="ts">
  import type { Mixer } from '$lib/mixer.svelte'

  /** A horizontal fader from A to B. Drag the cap or the track; double-click centres it. */
  let { mixer }: { mixer: Mixer } = $props()

  const KEY_STEP = 0.05

  let track = $state<HTMLDivElement>()
  let dragging = $state(false)

  function positionAt(clientX: number): number {
    const rect = track?.getBoundingClientRect()
    if (!rect || rect.width <= 0) return mixer.crossfader
    return (clientX - rect.left) / rect.width
  }

  function onpointerdown(e: PointerEvent): void {
    if (e.button !== 0) return
    e.preventDefault()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    dragging = true
    mixer.setCrossfader(positionAt(e.clientX))
  }

  function onpointermove(e: PointerEvent): void {
    if (dragging) mixer.setCrossfader(positionAt(e.clientX))
  }

  function end(): void {
    if (!dragging) return
    dragging = false
    mixer.commitCrossfader()
  }

  function onkeydown(e: KeyboardEvent): void {
    const step = e.key === 'ArrowLeft' ? -KEY_STEP : e.key === 'ArrowRight' ? KEY_STEP : 0
    const to = e.key === 'Home' ? 0 : e.key === 'End' ? 1 : null
    if (!step && to == null) return
    e.preventDefault()
    e.stopPropagation()
    mixer.setCrossfader(to ?? mixer.crossfader + step)
    mixer.commitCrossfader()
  }

  function centre(): void {
    mixer.setCrossfader(0.5)
    mixer.commitCrossfader()
  }
</script>

<div class="flex w-full items-center gap-2 text-[11px] font-semibold text-faint">
  <span class={mixer.crossfader <= 0.5 ? 'text-fg' : undefined}>A</span>
  <div
    bind:this={track}
    class="relative h-8 flex-1 cursor-pointer touch-none outline-none focus-visible:ring-1 focus-visible:ring-accent/60"
    role="slider"
    tabindex="0"
    aria-label="Crossfader"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={Math.round(mixer.crossfader * 100)}
    aria-valuetext={mixer.crossfader === 0.5
      ? 'Centre'
      : mixer.crossfader < 0.5
        ? `${Math.round((0.5 - mixer.crossfader) * 200)}% towards A`
        : `${Math.round((mixer.crossfader - 0.5) * 200)}% towards B`}
    title="Drag to fade between the decks. Double-click to centre."
    {onpointerdown}
    {onpointermove}
    onpointerup={end}
    onpointercancel={end}
    ondblclick={centre}
    {onkeydown}
  >
    <div class="absolute inset-x-0 top-1/2 h-1 -translate-y-1/2 rounded-full bg-raised"></div>
    <div class="absolute top-1/2 left-1/2 h-3 w-px -translate-y-1/2 bg-line"></div>
    <div
      class={[
        'absolute top-1/2 h-7 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-sm border shadow-md shadow-black/50',
        dragging ? 'border-accent bg-fg' : 'border-line bg-muted'
      ]}
      style:left="{mixer.crossfader * 100}%"
    >
      <div class="mx-auto mt-[11px] h-px w-2 bg-bg"></div>
    </div>
  </div>
  <span class={mixer.crossfader >= 0.5 ? 'text-fg' : undefined}>B</span>
</div>
