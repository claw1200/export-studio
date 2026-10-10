<script lang="ts">
  import { getApp } from '$lib/context'
  import type { GridDraft } from '$lib/beat'
  import { setupCanvas } from '$lib/waveform/canvas'
  import { drawDetail } from '$lib/waveform/detail'
  import { trackBandFall } from '$lib/waveform/fall'
  import { gridHandleIndex } from '$lib/waveform/markers'

  const { player } = getApp()

  let width = $state(0)
  let height = $state(0)
  let drag: { x: number; position: number } | null = null
  let gridDrag: { x: number; base: GridDraft } | null = null
  let movingGrid = $state(false)
  let scrubbing = $state(false)
  let pointer = $state.raw<{ x: number; y: number } | null>(null)

  const secondsPerPixel = (): number => (player.waveform ? 1 / (player.waveform.rate * player.display.zoom) : 0)

  function pointOf(e: PointerEvent): { x: number; y: number } | null {
    if (!(e.currentTarget instanceof HTMLElement)) return null
    const rect = e.currentTarget.getBoundingClientRect()
    return { x: e.clientX - rect.left, y: e.clientY - rect.top }
  }

  function handleAt(point: { x: number; y: number } | null): number {
    const wf = player.waveform
    if (!player.grid.editing || !wf || !point || !width || !height) return -1
    return gridHandleIndex(
      wf,
      player.position,
      player.display.zoom,
      player.displayGrid,
      width,
      height,
      point.x,
      point.y
    )
  }

  const hotGridIndex = $derived(scrubbing ? -1 : handleAt(pointer))

  function paint(canvas: HTMLCanvasElement): void {
    trackBandFall()
    if (!width || !height) return
    const ctx = setupCanvas(canvas, width, height)
    const wf = player.waveform
    if (!wf) {
      ctx.clearRect(0, 0, canvas.width, canvas.height)
      return
    }
    drawDetail(
      ctx,
      wf,
      player.display.mode,
      {
        position: player.position,
        zoom: player.display.zoom,
        cues: player.displayCues,
        beatgrid: player.displayGrid,
        cuePoint: player.track ? player.cuePoint : null,
        loop: player.loop,
        gridEditing: player.grid.editing,
        hotGridIndex
      },
      canvas.width,
      canvas.height,
      window.devicePixelRatio || 1,
      player.display.paintSamples,
      player.display.paintQuality,
      player.display.paintFall
    )
  }

  function onwheel(e: WheelEvent): void {
    if (!player.waveform) return
    e.preventDefault()
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    player.display.setZoom(player.display.zoom * Math.exp(-delta * 0.01))
  }

  function release(): void {
    gridDrag = null
    movingGrid = false
    scrubbing = false
    if (!drag) return
    drag = null
    player.endScrub()
  }
</script>

<div
  class={[
    'relative h-full w-full touch-none',
    movingGrid || hotGridIndex >= 0 ? 'cursor-ew-resize' : 'cursor-grab active:cursor-grabbing'
  ]}
  bind:clientWidth={width}
  bind:clientHeight={height}
  role="slider"
  tabindex="-1"
  aria-label={player.grid.editing ? 'Waveform. Drag a beat handle to move the grid.' : 'Waveform'}
  aria-valuenow={Math.round(player.position)}
  {onwheel}
  onpointerdown={(e) => {
    if (!player.waveform || e.button !== 0) return
    const point = pointOf(e)
    if (!point) return
    e.currentTarget.setPointerCapture(e.pointerId)
    pointer = point
    if (player.grid.draft && handleAt(point) >= 0) {
      gridDrag = { x: e.clientX, base: player.grid.draft }
      movingGrid = true
      return
    }
    drag = { x: e.clientX, position: player.position }
    scrubbing = true
    player.scrub(drag.position, e.timeStamp)
  }}
  onpointermove={(e) => {
    const point = pointOf(e)
    if (point) pointer = point
    if (gridDrag) {
      player.grid.nudge((e.clientX - gridDrag.x) * secondsPerPixel() * 1000, gridDrag.base)
      return
    }
    if (!drag) return
    player.scrub(drag.position - (e.clientX - drag.x) * secondsPerPixel(), e.timeStamp)
  }}
  onpointerleave={() => {
    if (!gridDrag && !drag) pointer = null
  }}
  onpointerup={release}
  onpointercancel={release}
  onlostpointercapture={release}
>
  <canvas {@attach paint} class="absolute inset-0 size-full"></canvas>
</div>
