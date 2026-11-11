import type { WaveSamples } from '../deck/waves.svelte'
import { OVERVIEW_TOP_PAD, drawRgbOverview, drawStackedBands } from './analysis'
import { drawAtHeightScale } from './canvas'
import type { Waveform, WaveformMode } from './data'
import { drawSampleWave } from './samples'

/** The whole track across the canvas. `samples` draws a file that has no rekordbox analysis. */
export function drawOverview(
  ctx: CanvasRenderingContext2D,
  wf: Waveform,
  mode: WaveformMode,
  pw: number,
  ph: number,
  samples: WaveSamples | null = null,
  rgbColors = wf.rgbColors,
  heightScale = 1
): void {
  ctx.clearRect(0, 0, pw, ph)
  if (samples && samples.data.length > 1) {
    const sampleBands = mode === '3band' && samples.bands && samples.bands.length >= 6 ? samples.bands : null
    if (sampleBands) drawAtHeightScale(ctx, ph, heightScale, 'bottom', (h) => drawStackedBands(ctx, sampleBands, pw, h))
    else
      drawAtHeightScale(ctx, ph, heightScale, 'center', (h) =>
        drawSampleWave(ctx, wf, samples, rgbColors, 0, wf.length, pw, h, 100, OVERVIEW_TOP_PAD)
      )
    return
  }
  const bands = mode === '3band' ? (wf.previewBands ?? wf.bands) : null
  drawAtHeightScale(ctx, ph, heightScale, 'bottom', (h) => {
    if (bands) drawStackedBands(ctx, bands, pw, h)
    else if (wf.rgbHeights && rgbColors) drawRgbOverview(ctx, wf.rgbHeights, rgbColors, wf.length, pw, h)
  })
}
