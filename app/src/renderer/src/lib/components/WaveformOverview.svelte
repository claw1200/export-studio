<script lang="ts">
  import { getApp } from '$lib/context'
  import { setupCanvas } from '$lib/waveform/canvas'
  import { CUE_POINT_COLOR } from '$lib/waveform/colors'
  import { drawOverview } from '$lib/waveform/overview'
  import WaveformMarks from './WaveformMarks.svelte'

  const { player } = getApp()

  let width = $state(0)
  let height = $state(0)

  const span = $derived(player.waveform ? player.waveform.length / player.waveform.rate : player.duration || 1)
  const pct = (seconds: number): number => Math.max(0, Math.min(100, (seconds / span) * 100))
  const lineAt = $derived(player.previewAt ?? player.position)

  function paint(canvas: HTMLCanvasElement): void {
    if (!width || !height) return
    const ctx = setupCanvas(canvas, width, height)
    if (player.waveform)
      drawOverview(
        ctx,
        player.waveform,
        player.display.mode,
        canvas.width,
        canvas.height,
        player.display.prepared ? player.display.samples : null
      )
    else ctx.clearRect(0, 0, canvas.width, canvas.height)
  }

  const WAVEFORM_PREVIEW = 'waveform'
  let rawWheelSeconds: number | null = null

  function secondsAt(e: PointerEvent): number {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left))
    return player.quantised((x / Math.max(rect.width, 1)) * span)
  }

  function drag(e: PointerEvent): void {
    rawWheelSeconds = null
    player.scrub(secondsAt(e), e.timeStamp)
  }

  function preview(e: PointerEvent): void {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const inside =
      e.clientX >= rect.left && e.clientX <= rect.right && e.clientY >= rect.top && e.clientY <= rect.bottom
    const overCue = e.target instanceof Element && e.target.closest('button')
    if (inside && !overCue) player.setPreview(WAVEFORM_PREVIEW, secondsAt(e))
    else player.clearPreview(WAVEFORM_PREVIEW)
  }

  function onwheel(e: WheelEvent): void {
    if (!player.waveform || !width) return
    e.preventDefault()
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    const at = Math.max(0, Math.min(span, (rawWheelSeconds ?? player.position) + delta * (span / width)))
    rawWheelSeconds = at
    player.seek(player.quantised(at))
  }
</script>

<div
  class="relative h-full w-full cursor-ew-resize overflow-hidden bg-black/30"
  bind:clientWidth={width}
  bind:clientHeight={height}
  role="slider"
  tabindex="-1"
  aria-label="Track position"
  aria-valuemin={0}
  aria-valuemax={Math.round(span)}
  aria-valuenow={Math.round(player.position)}
  {onwheel}
  onpointerdown={(e) => {
    player.clearPreview(WAVEFORM_PREVIEW)
    e.currentTarget.setPointerCapture(e.pointerId)
    rawWheelSeconds = null
    player.seek(secondsAt(e))
  }}
  onpointermove={(e) => {
    if (e.currentTarget.hasPointerCapture(e.pointerId)) drag(e)
    else preview(e)
  }}
  onpointerleave={() => player.clearPreview(WAVEFORM_PREVIEW)}
  onpointerup={(e) => {
    player.endScrub()
    preview(e)
  }}
  onpointercancel={() => {
    player.clearPreview(WAVEFORM_PREVIEW)
    player.endScrub()
  }}
>
  <canvas {@attach paint} class="absolute inset-0 size-full"></canvas>
  <div class="pointer-events-none absolute inset-y-0 left-0 bg-black/50" style:width="{pct(player.position)}%"></div>

  <WaveformMarks
    cues={player.displayCues}
    loop={player.loop}
    {pct}
    onpointerdown={(_cue, e) => e.stopPropagation()}
    onclick={(cue) => (cue.kind === 'memory' ? player.callCue(cue) : player.seek(cue.start / 1000))}
  />

  {#if player.track}
    <span
      class="pointer-events-none absolute bottom-0 z-10 h-[7px] w-[9px] -translate-x-1/2"
      style:left="{pct(player.cuePoint)}%"
      style:background={CUE_POINT_COLOR}
      style:clip-path="polygon(50% 0, 100% 100%, 0 100%)"
    ></span>
  {/if}

  <div
    class="pointer-events-none absolute inset-y-0 w-0.5 -translate-x-1/2 bg-white shadow-[0_0_6px_rgba(255,255,255,0.6)]"
    style:left="{pct(lineAt)}%"
  ></div>
</div>
