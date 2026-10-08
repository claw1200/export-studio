import type { TrackDetail } from '../types'

export type WaveformMode = 'rgb' | '3band'

/** Rekordbox's waveform analysis for a track, decoded. */
export interface Waveform {
  rate: number
  length: number
  rgbHeights?: Uint8Array
  rgbColors?: Uint8Array
  bands?: Uint8Array
  previewBands?: Uint8Array
}

function b64(data: string): Uint8Array {
  const bin = atob(data)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

export function decodeWaveform(detail: TrackDetail): Waveform | null {
  const { rate, rgb, threeBand, threeBandPreview, mono } = detail.waveform
  const wf: Waveform = { rate, length: 0 }
  if (rgb) {
    wf.rgbHeights = b64(rgb.heights)
    wf.rgbColors = b64(rgb.colors)
    wf.length = rgb.length
  } else if (mono) {
    wf.rgbHeights = b64(mono.heights)
    const white = b64(mono.whiteness)
    const colors = new Uint8Array(white.length * 3)
    for (let i = 0; i < white.length; i++) {
      const w = white[i] / 255
      colors[i * 3] = 40 + 180 * w
      colors[i * 3 + 1] = 110 + 130 * w
      colors[i * 3 + 2] = 255
    }
    wf.rgbColors = colors
    wf.length = mono.length
  }
  if (threeBand) {
    wf.bands = b64(threeBand.bands)
    wf.length = Math.max(wf.length, threeBand.length)
  }
  if (threeBandPreview) wf.previewBands = b64(threeBandPreview.bands)
  return wf.length ? wf : null
}
