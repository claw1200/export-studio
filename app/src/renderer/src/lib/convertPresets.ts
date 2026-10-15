import type { ConvertFormat, ConvertPreset, Track } from './types'

export const CONVERT_PRESETS_KEY = 'convertPresets'

export const FORMAT_NAMES: Record<ConvertFormat, string> = {
  wav: 'WAV',
  aiff: 'AIFF',
  flac: 'FLAC',
  alac: 'ALAC',
  mp3: 'MP3',
  aac: 'AAC'
}

const LOSSLESS: ConvertFormat[] = ['wav', 'aiff', 'flac', 'alac']
export const FORMATS = Object.keys(FORMAT_NAMES) as ConvertFormat[]
export const BIT_DEPTHS = [16, 24]
export const SAMPLE_RATES = [44100, 48000, 88200, 96000]
export const BITRATES = [96, 128, 160, 192, 224, 256, 320]

export const DEFAULT_PRESETS: ConvertPreset[] = [
  { format: 'wav', bitDepth: 16, sampleRate: 48000 },
  { format: 'flac', bitDepth: 16, sampleRate: 48000 },
  { format: 'mp3', bitrate: 320 },
  { format: 'aac', bitrate: 256 },
  { format: 'alac', bitDepth: 16, sampleRate: 48000 }
]

export function isLossless(format: ConvertFormat): boolean {
  return LOSSLESS.includes(format)
}

export function presetLabel(preset: ConvertPreset): string {
  const name = FORMAT_NAMES[preset.format]
  if (!isLossless(preset.format)) return `${name} ${preset.bitrate} kbps`
  const rate = (preset.sampleRate ?? 0) / 1000
  return `${name} ${preset.bitDepth}-bit ${Number.isInteger(rate) ? rate : rate.toFixed(1)} kHz`
}

export function presetKey(preset: ConvertPreset): string {
  return [preset.format, preset.bitDepth, preset.sampleRate, preset.bitrate].join(':')
}

function valid(raw: unknown): ConvertPreset | null {
  if (!raw || typeof raw !== 'object') return null
  const { format, bitDepth, sampleRate, bitrate } = raw as Record<string, unknown>
  if (typeof format !== 'string' || !FORMATS.includes(format as ConvertFormat)) return null
  const f = format as ConvertFormat
  if (isLossless(f)) {
    if (!BIT_DEPTHS.includes(bitDepth as number) || !SAMPLE_RATES.includes(sampleRate as number)) return null
    return { format: f, bitDepth: bitDepth as number, sampleRate: sampleRate as number }
  }
  if (!BITRATES.includes(bitrate as number)) return null
  return { format: f, bitrate: bitrate as number }
}

export function parsePresets(raw: string | null): ConvertPreset[] {
  if (raw == null) return DEFAULT_PRESETS
  try {
    const list = JSON.parse(raw)
    if (!Array.isArray(list)) return DEFAULT_PRESETS
    const seen = new Set<string>()
    return list
      .map(valid)
      .filter((p): p is ConvertPreset => !!p)
      .filter((p) => !seen.has(presetKey(p)) && !!seen.add(presetKey(p)))
  } catch {
    return DEFAULT_PRESETS
  }
}

/** Whether a track is already stored in `preset`'s format with matching quality. */
export function matchesPreset(track: Track, preset: ConvertPreset): boolean {
  const type = track.fileType.toUpperCase()
  if (type !== FORMAT_NAMES[preset.format] && !(type === 'M4A' && (preset.format === 'aac' || preset.format === 'alac')))
    return false
  if (isLossless(preset.format)) return track.bitDepth === preset.bitDepth && track.sampleRate === preset.sampleRate
  return Math.round(track.bitrate) === preset.bitrate
}
