<script lang="ts">
  import { FADER_CENTRE } from '$lib/crossfade'
  import type { Mixer } from '$lib/mixer.svelte'

  let { mixer }: { mixer: Mixer } = $props()

  const KEY_STEP = 0.05

  const offCentrePercent = $derived(Math.round((mixer.crossfader - FADER_CENTRE) * 200))

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
    mixer.saveCrossfader()
  }

  function setAndSave(position: number): void {
    mixer.setCrossfader(position)
    mixer.saveCrossfader()
  }

  function onkeydown(e: KeyboardEvent): void {
    const step = e.key === 'ArrowLeft' ? -KEY_STEP : e.key === 'ArrowRight' ? KEY_STEP : 0
    const to = e.key === 'Home' ? 0 : e.key === 'End' ? 1 : null
    if (!step && to == null) return
    e.preventDefault()
    e.stopPropagation()
    setAndSave(to ?? mixer.crossfader + step)
  }

  const WHEEL_NOTCH_PX = 100
  const TRAVEL_PER_WHEEL_NOTCH = 0.1
  const TRAVEL_PER_WHEEL_PX = TRAVEL_PER_WHEEL_NOTCH / WHEEL_NOTCH_PX
  const WHEEL_SAVE_DELAY_MS = 300
  let wheelSave: ReturnType<typeof setTimeout> | undefined
  // Small trackpad deltas would each land inside the centre snap and be pulled back to it.
  let wheelPositionBeforeSnap: number | null = null

  function onwheel(e: WheelEvent): void {
    e.preventDefault()
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    const px = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? delta * 16 : delta
    wheelPositionBeforeSnap = Math.min(
      1,
      Math.max(0, (wheelPositionBeforeSnap ?? mixer.crossfader) + px * TRAVEL_PER_WHEEL_PX)
    )
    mixer.setCrossfader(wheelPositionBeforeSnap)
    clearTimeout(wheelSave)
    wheelSave = setTimeout(() => {
      wheelPositionBeforeSnap = null
      mixer.saveCrossfader()
    }, WHEEL_SAVE_DELAY_MS)
  }

  function centre(): void {
    setAndSave(FADER_CENTRE)
  }
</script>

<div class="flex w-full items-center gap-2 text-[11px] font-semibold text-faint">
  <span class={mixer.crossfader <= FADER_CENTRE ? 'text-fg' : undefined}>A</span>
  <div
    bind:this={track}
    class="relative h-8 flex-1 cursor-pointer touch-none outline-none focus-visible:ring-1 focus-visible:ring-accent/60"
    role="slider"
    tabindex="0"
    aria-label="Crossfader"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={Math.round(mixer.crossfader * 100)}
    aria-valuetext={offCentrePercent === 0
      ? 'Centre'
      : `${Math.abs(offCentrePercent)}% towards ${offCentrePercent < 0 ? 'A' : 'B'}`}
    title="Drag or scroll to fade between the decks. Double-click to centre."
    {onpointerdown}
    {onpointermove}
    onpointerup={end}
    onpointercancel={end}
    ondblclick={centre}
    {onkeydown}
    {onwheel}
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
  <span class={mixer.crossfader >= FADER_CENTRE ? 'text-fg' : undefined}>B</span>
</div>
