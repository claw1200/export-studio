/** The scrolling waveform around the playhead, with everything drawn over it. */
import type { WaveSamples } from '../deck/waves.svelte'
import { drawAnalysisBands, drawRgbDetail } from './analysis'
import { drawAtHeightScale } from './canvas'
import type { Waveform, WaveformMode } from './data'
import type { DetailView, Frame } from './frame'
import {
  paintBeatCounters,
  paintBeatGrid,
  paintCuePoint,
  paintCues,
  paintGridHandles,
  paintLoopEdges,
  paintLoops,
  paintPlayhead
} from './markers'
import { drawSampleWave, waveStride } from './samples'

function visibleRange(wf: Waveform, view: DetailView, cssWidth: number): [number, number] {
  const playheadEntry = view.position * wf.rate
  const entriesInView = cssWidth / view.zoom
  return [playheadEntry - entriesInView * view.playheadAt, playheadEntry + entriesInView * (1 - view.playheadAt)]
}

function paintWave(
  frame: Frame,
  mode: WaveformMode,
  samples: WaveSamples | null | undefined,
  quality: number,
  fallBlocks: number,
  rgbColors: Uint8Array | undefined,
  heightScale: number
): void {
  const { ctx, wf, from, to, pw, ph, dpr } = frame
  ctx.save()
  ctx.translate(0, 9 * dpr)
  drawAtHeightScale(ctx, ph - 18 * dpr, heightScale, 'center', (waveH) => {
    const custom = quality > 0 ? samples : null
    if (mode === 'rgb' && custom) drawSampleWave(ctx, wf, custom, rgbColors, from, to, pw, waveH, quality)
    else if (mode === '3band' && custom?.bands && custom.bandRate) {
      const stride = waveStride(quality)
      const blocksPerSecond = custom.bandRate / stride
      const scale = blocksPerSecond / wf.rate
      const fall = fallBlocks >= 1 ? { blocksPerSecond, timeConstantBlocks: fallBlocks } : null
      drawAnalysisBands(ctx, custom.bands, from * scale, to * scale, pw, waveH, 'center', stride, fall)
    } else if (mode === '3band' && wf.bands) drawAnalysisBands(ctx, wf.bands, from, to, pw, waveH, 'center')
    else if (wf.rgbHeights && rgbColors) drawRgbDetail(ctx, wf.rgbHeights, rgbColors, from, to, pw, waveH)
  })
  ctx.restore()
}

export function drawDetail(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  mode: WaveformMode,
  view: DetailView,
  pw: number,
  ph: number,
  dpr: number,
  samples?: WaveSamples | null,
  quality = 0,
  fallBlocks = 0,
  rgbColors = wf.rgbColors,
  heightScale = 1
): void {
  ctx.clearRect(0, 0, pw, ph)
  const [from, to] = visibleRange(wf, view, pw / dpr)
  const frame: Frame = {
    ctx,
    wf,
    view,
    pw,
    ph,
    dpr,
    from,
    to,
    playheadX: Math.round(pw * view.playheadAt),
    toX: (seconds) => ((seconds * wf.rate - from) / (to - from)) * pw
  }
  paintLoops(frame)
  paintWave(frame, mode, samples, quality, fallBlocks, rgbColors, heightScale)
  paintLoopEdges(frame)
  paintBeatGrid(frame)
  paintCues(frame)
  paintCuePoint(frame)
  paintGridHandles(frame)
  paintPlayhead(frame)
  paintBeatCounters(frame)
}
