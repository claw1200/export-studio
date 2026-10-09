/** The scrolling waveform around the playhead, with everything drawn over it. */
import type { WaveSamples } from '../deck/waves.svelte'
import { drawAnalysisBands, drawAnalysisRgb } from './analysis'
import type { Waveform, WaveformMode } from './data'
import type { DetailView, Frame } from './frame'
import {
  paintBeatCounters,
  paintBeatGrid,
  paintCuePoint,
  paintCues,
  paintGridHandles,
  paintLoops,
  paintPlayhead
} from './markers'
import { drawSampleWave, waveStride } from './samples'

/** The analysis entries in view: `cssWidth` pixels centred on the playhead. */
function visibleRange(wf: Waveform, view: DetailView, cssWidth: number): [number, number] {
  const center = view.position * wf.rate
  const halfEntries = cssWidth / 2 / view.zoom
  return [center - halfEntries, center + halfEntries]
}

/**
 * Each analysis entry is the peak of the window that starts at its index, so drawing it from that start puts
 * transients up to one entry early. Rekordbox draws an entry at the end of its window.
 */
const ANALYSIS_LAG_ENTRIES = 0

function paintWave(
  frame: Frame,
  mode: WaveformMode,
  samples: WaveSamples | null | undefined,
  quality: number,
  fallBlocks: number
): void {
  const { ctx, wf, from, to, pw, ph, dpr } = frame
  const waveH = ph - 18 * dpr
  const entryFrom = from - ANALYSIS_LAG_ENTRIES
  const entryTo = to - ANALYSIS_LAG_ENTRIES
  ctx.save()
  ctx.translate(0, 9 * dpr)
  const custom = quality > 0 ? samples : null
  if (mode === 'rgb' && custom) drawSampleWave(ctx, wf, custom, from, to, pw, waveH, quality)
  else if (mode === '3band' && custom?.bands && custom.bandRate) {
    const stride = waveStride(quality)
    const scale = custom.bandRate / stride / wf.rate
    drawAnalysisBands(ctx, custom.bands, entryFrom * scale, entryTo * scale, pw, waveH, 'center', stride, fallBlocks)
  } else if (mode === '3band' && wf.bands) drawAnalysisBands(ctx, wf.bands, entryFrom, entryTo, pw, waveH, 'center')
  else if (wf.rgbHeights && wf.rgbColors)
    drawAnalysisRgb(ctx, wf.rgbHeights, wf.rgbColors, entryFrom, entryTo, pw, waveH, 'center')
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
  fallBlocks = 0
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
    toX: (seconds) => ((seconds * wf.rate - from) / (to - from)) * pw
  }
  paintLoops(frame)
  paintWave(frame, mode, samples, quality, fallBlocks)
  paintBeatGrid(frame)
  paintCues(frame)
  paintCuePoint(frame)
  paintGridHandles(frame)
  paintPlayhead(frame)
  paintBeatCounters(frame)
}
