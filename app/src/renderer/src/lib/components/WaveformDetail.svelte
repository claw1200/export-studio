<script lang="ts">
  import { getApp } from '$lib/context'
  import { drawDetail, setupCanvas } from '$lib/waveform'

  const { player } = getApp()

  let width = $state(0)
  let height = $state(0)
  let drag: { x: number; position: number } | null = null

  function paint(canvas: HTMLCanvasElement): void {
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
        cues: player.detail?.cues ?? [],
        beatgrid: player.detail?.beatgrid ?? null
      },
      canvas.width,
      canvas.height,
      window.devicePixelRatio || 1
    )
  }

  const secondsPerPixel = (): number => (player.waveform ? 1 / (player.waveform.rate * player.zoom) : 0)

  function onwheel(e: WheelEvent): void {
    if (!player.waveform) return
    e.preventDefault()
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    player.setZoom(player.zoom * Math.exp(-delta * 0.01))
  }
</script>

<div
  class="relative h-full w-full cursor-grab touch-none active:cursor-grabbing"
  bind:clientWidth={width}
  bind:clientHeight={height}
  role="slider"
  tabindex="-1"
  aria-label="Waveform"
  aria-valuenow={Math.round(player.position)}
  {onwheel}
  onpointerdown={(e) => {
    if (!player.waveform) return
    e.currentTarget.setPointerCapture(e.pointerId)
    drag = { x: e.clientX, position: player.position }
  }}
  onpointermove={(e) => {
    if (!drag) return
    player.scrub(drag.position - (e.clientX - drag.x) * secondsPerPixel())
  }}
  onpointerup={() => {
    if (!drag) return
    player.commit()
    drag = null
  }}
  onpointercancel={() => {
    if (drag) player.commit()
    drag = null
  }}
>
  <canvas {@attach paint} class="absolute inset-0 size-full"></canvas>
</div>
