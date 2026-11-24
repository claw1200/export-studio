import type { WaveSamples } from '../deck/waves.svelte'
import { drawRgbOverview, drawStackedBands } from './analysis'
import { drawAtHeightScale } from './canvas'
import type { Waveform, WaveformMode } from './data'
import { pendingColors } from './gradientColors'
import { sampleHeights } from './samples'

/** The whole track across the canvas, from rekordbox's analysis or, given `samples`, the deck's own scan. */
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
  const scan = samples && samples.data.length > 1 ? samples : null
  const bands = mode === '3band' ? (scan ? scan.bands : (wf.previewBands ?? wf.bands)) : null
  drawAtHeightScale(ctx, ph, heightScale, 'bottom', (h) => {
    if (bands && bands.length >= 6) drawStackedBands(ctx, bands, pw, h)
    else if (scan) {
      const heights = sampleHeights(scan, wf.rate, wf.length)
      drawRgbOverview(ctx, heights, rgbColors ?? pendingColors(wf.length), wf.length, pw, h)
    } else if (wf.rgbHeights && rgbColors) drawRgbOverview(ctx, wf.rgbHeights, rgbColors, wf.length, pw, h)
  })
}
