<script lang="ts">
  import type { BrowserWave } from '$lib/browserPreview.svelte'
  import { getApp } from '$lib/context'
  import type { Cue, Track } from '$lib/types'
  import { drawAnalysisRgb, drawStackedBands } from '$lib/waveform/analysis'
  import { setupCanvas } from '$lib/waveform/canvas'
  import WaveformMarks from './WaveformMarks.svelte'

  let { track, wave, cues }: { track: Track; wave: BrowserWave | null; cues: Cue[] } = $props()

  const { player, audition } = getApp()

  let width = $state(0)
  let height = $state(0)
  let hover = $state<number | null>(null)
  let holding = $state(false)

  const span = $derived(track.duration || 1)
  const head = $derived(audition.trackId === track.id ? audition.position : null)
  const loop = $derived(audition.trackId === track.id ? audition.loop : null)
  const line = $derived(holding ? head : (hover ?? head))

  function releaseOnRemove(): () => void {
    return () => {
      if (holding) audition.stop()
    }
  }

  function paint(canvas: HTMLCanvasElement): void {
    if (!width || !height || !wave) return
    const ctx = setupCanvas(canvas, width, height)
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    const bands = player.display.mode === '3band' ? wave.bands : null
    if (bands) drawStackedBands(ctx, bands, canvas.width, canvas.height)
    else if (wave.heights && wave.colors)
      drawAnalysisRgb(ctx, wave.heights, wave.colors, 0, wave.rgbLength, canvas.width, canvas.height, 'bottom')
    else if (wave.bands) drawStackedBands(ctx, wave.bands, canvas.width, canvas.height)
  }

  function secondsAt(e: PointerEvent): number {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left))
    return (x / Math.max(rect.width, 1)) * span
  }

  function pct(seconds: number): number {
    return Math.max(0, Math.min(100, (seconds / span) * 100))
  }
</script>

{#if wave}
  <div
    class="relative h-[85%] max-h-[85%] w-full min-w-0 cursor-pointer overflow-hidden bg-black/40"
    {@attach releaseOnRemove}
    bind:clientWidth={width}
    bind:clientHeight={height}
    role="button"
    tabindex="-1"
    aria-label="Preview {track.title}"
    aria-pressed={holding}
    title="Hold to preview"
    onmousedown={(e) => {
      e.preventDefault()
      e.stopPropagation()
    }}
    ondblclick={(e) => e.stopPropagation()}
    ondragstart={(e) => {
      e.preventDefault()
      e.stopPropagation()
    }}
    onpointerdown={(e) => {
      if (e.button !== 0) return
      e.stopPropagation()
      e.currentTarget.setPointerCapture(e.pointerId)
      holding = true
      hover = null
      audition.playAt(track, secondsAt(e))
    }}
    onpointermove={(e) => {
      if (e.currentTarget.hasPointerCapture(e.pointerId)) return
      hover = secondsAt(e)
    }}
    onpointerup={(e) => {
      if (!holding) return
      holding = false
      audition.stop()
      const rect = e.currentTarget.getBoundingClientRect()
      const inside =
        e.clientX >= rect.left && e.clientX <= rect.right && e.clientY >= rect.top && e.clientY <= rect.bottom
      hover = inside ? secondsAt(e) : null
    }}
    onpointerleave={() => {
      if (!holding) hover = null
    }}
    onpointercancel={() => {
      if (!holding) return
      holding = false
      hover = null
      audition.stop()
    }}
  >
    <canvas {@attach paint} draggable="false" class="pointer-events-none absolute inset-0 size-full" aria-hidden="true"
    ></canvas>
    {#if head != null}
      <div class="pointer-events-none absolute inset-y-0 left-0 bg-black/45" style:width="{pct(head)}%"></div>
    {/if}
    <WaveformMarks
      {cues}
      {loop}
      {pct}
      draggable={false}
      onpointerdown={(cue, e) => {
        if (e.button !== 0) return
        e.stopPropagation()
        e.preventDefault()
        if (e.currentTarget instanceof HTMLButtonElement) e.currentTarget.setPointerCapture(e.pointerId)
        holding = true
        hover = null
        audition.playCue(track, cue)
      }}
      onpointerup={(_cue, e) => {
        e.stopPropagation()
        if (!holding) return
        holding = false
        audition.stop()
      }}
      onpointercancel={(_cue, e) => {
        e.stopPropagation()
        if (!holding) return
        holding = false
        audition.stop()
      }}
      onclick={(_cue, e) => e.stopPropagation()}
    />
    {#if line != null}
      <div
        class="pointer-events-none absolute inset-y-0 w-px -translate-x-1/2 bg-white"
        style:left="{pct(line)}%"
      ></div>
    {/if}
  </div>
{/if}
