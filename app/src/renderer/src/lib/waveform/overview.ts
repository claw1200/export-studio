import type { WaveSamples } from '../deck/waves.svelte'
import { drawAnalysisRgb, drawStackedBands } from './analysis'
import type { Waveform, WaveformMode } from './data'
import { drawSampleWave } from './samples'

/** The whole track across the canvas. `samples` draws a file that has no rekordbox analysis. */
export function drawOverview(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  mode: WaveformMode,
  pw: number,
  ph: number,
  samples: WaveSamples | null = null
): void {
  ctx.clearRect(0, 0, pw, ph)
  if (samples && samples.data.length > 1) {
    if (mode === '3band' && samples.bands && samples.bands.length >= 6) drawStackedBands(ctx, samples.bands, pw, ph)
    else drawSampleWave(ctx, wf, samples, 0, wf.length, pw, ph, 100)
    return
  }
  const bands = mode === '3band' ? (wf.previewBands ?? wf.bands) : null
  if (bands) drawStackedBands(ctx, bands, pw, ph)
  else if (wf.rgbHeights && wf.rgbColors)
    drawAnalysisRgb(ctx, wf.rgbHeights, wf.rgbColors, 0, wf.length, pw, ph, 'bottom')
}
