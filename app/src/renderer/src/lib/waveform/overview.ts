import { drawAnalysisRgb, drawStackedBands } from './analysis'
import type { Waveform, WaveformMode } from './data'

/** The whole track across the canvas. */
export function drawOverview(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  mode: WaveformMode,
  pw: number,
  ph: number
): void {
  ctx.clearRect(0, 0, pw, ph)
  const bands = mode === '3band' ? (wf.previewBands ?? wf.bands) : null
  if (bands) drawStackedBands(ctx, bands, pw, ph)
  else if (wf.rgbHeights && wf.rgbColors)
    drawAnalysisRgb(ctx, wf.rgbHeights, wf.rgbColors, 0, wf.length, pw, ph, 'bottom')
}
