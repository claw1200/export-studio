<script lang="ts">
  import { getApp } from '$lib/context'
  import type { GridDraft } from '$lib/beat'
  import { drawDetail, setupCanvas, trackBandFall } from '$lib/waveform'

  const { player } = getApp()

  let width = $state(0)
  let height = $state(0)
  let drag: { x: number; position: number } | null = null
  let gridDrag: { x: number; base: GridDraft } | null = null

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
      player.mode,
      {
        position: player.position,
        zoom: player.zoom,
        cues: player.displayCues,
        beatgrid: player.displayGrid,
        cuePoint: player.track ? player.cuePoint : null,
        gridEditing: player.gridEditing
      },
      canvas.width,
      canvas.height,
      window.devicePixelRatio || 1,
      player.waveQuality > 0 ? player.wave : null,
      player.waveQuality,
      player.fallBlocks
    )
  }

  const secondsPerPixel = (): number => (player.waveform ? 1 / (player.waveform.rate * player.zoom) : 0)

  function onwheel(e: WheelEvent): void {
    if (!player.waveform) return
    e.preventDefault()
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    player.setZoom(player.zoom * Math.exp(-delta * 0.01))
  }

  function release(): void {
    gridDrag = null
    if (!drag) return
    drag = null
    player.endScrub()
  }
</script>

<div
  class={[
    'relative h-full w-full touch-none',
    player.gridEditing ? 'cursor-ew-resize' : 'cursor-grab active:cursor-grabbing'
  ]}
  bind:clientWidth={width}
  bind:clientHeight={height}
  role="slider"
  tabindex="-1"
  aria-label={player.gridEditing ? 'Beatgrid' : 'Waveform'}
  aria-valuenow={Math.round(player.position)}
  {onwheel}
  onpointerdown={(e) => {
    if (!player.waveform || e.button !== 0) return
    e.currentTarget.setPointerCapture(e.pointerId)
    if (player.gridEditing) {
      if (player.gridDraft) gridDrag = { x: e.clientX, base: player.gridDraft }
      return
    }
    drag = { x: e.clientX, position: player.position }
    player.scrub(drag.position)
  }}
  onpointermove={(e) => {
    if (gridDrag) {
      player.nudgeGrid((e.clientX - gridDrag.x) * secondsPerPixel() * 1000, gridDrag.base)
      return
    }
    if (!drag) return
    player.scrub(drag.position - (e.clientX - drag.x) * secondsPerPixel())
  }}
  onpointerup={release}
  onpointercancel={release}
  onlostpointercapture={release}
>
  <canvas {@attach paint} class="absolute inset-0 size-full"></canvas>
</div>
