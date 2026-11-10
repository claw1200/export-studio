import { getSetting, setSetting } from '../settings'

export const BAND_NAMES_LOW_TO_HIGH = ['Low', 'Mid', 'High'] as const

export const BLEND_MODES: { id: GlobalCompositeOperation; label: string }[] = [
  { id: 'source-over', label: 'Normal' },
  { id: 'screen', label: 'Screen' },
  { id: 'lighter', label: 'Add' },
  { id: 'multiply', label: 'Multiply' },
  { id: 'overlay', label: 'Overlay' },
  { id: 'soft-light', label: 'Soft light' },
  { id: 'hard-light', label: 'Hard light' },
  { id: 'color-dodge', label: 'Color dodge' },
  { id: 'lighten', label: 'Lighten' },
  { id: 'difference', label: 'Difference' }
]

interface BandStyleValues {
  colors: string[]
  layeredOpacities: number[]
  blend: GlobalCompositeOperation
}

export const DEFAULT_BAND_STYLE: Readonly<BandStyleValues> = {
  colors: ['#0054e1', '#f7a102', '#ffffff'],
  layeredOpacities: [1, 0.85, 0.85],
  blend: 'hard-light'
}

const KEY = 'waveformBandStyle'
const HEX = /^#[0-9a-f]{6}$/

function isBandList<T>(value: unknown, valid: (item: unknown) => item is T): value is T[] {
  return Array.isArray(value) && value.length === BAND_NAMES_LOW_TO_HIGH.length && value.every(valid)
}

const isHex = (value: unknown): value is string => typeof value === 'string' && HEX.test(value)
const isOpacity = (value: unknown): value is number => typeof value === 'number' && value >= 0 && value <= 1
const isBlend = (value: unknown): value is GlobalCompositeOperation => BLEND_MODES.some((mode) => mode.id === value)

function stored(): BandStyleValues {
  let saved: Partial<BandStyleValues> = {}
  try {
    saved = JSON.parse(getSetting(KEY) ?? '{}')
  } catch {
    saved = {}
  }
  return {
    colors: isBandList(saved.colors, isHex) ? saved.colors : [...DEFAULT_BAND_STYLE.colors],
    layeredOpacities: isBandList(saved.layeredOpacities, isOpacity)
      ? saved.layeredOpacities
      : [...DEFAULT_BAND_STYLE.layeredOpacities],
    blend: isBlend(saved.blend) ? saved.blend : DEFAULT_BAND_STYLE.blend
  }
}

/** How the 3-band waveforms are coloured and layered, kept between sessions. */
class BandStyle {
  private values = $state(stored())

  get colors(): readonly string[] {
    return this.values.colors
  }

  /** Where a band overlaps the bands beneath it; elsewhere it is drawn solid. */
  get layeredOpacities(): readonly number[] {
    return this.values.layeredOpacities
  }

  get blend(): GlobalCompositeOperation {
    return this.values.blend
  }

  get isDefault(): boolean {
    return JSON.stringify(this.values) === JSON.stringify(DEFAULT_BAND_STYLE)
  }

  setColor(band: number, color: string): void {
    const hex = color.toLowerCase()
    if (!isHex(hex)) return
    this.values.colors[band] = hex
    this.save()
  }

  setLayeredOpacity(band: number, opacity: number): void {
    this.values.layeredOpacities[band] = Math.min(1, Math.max(0, opacity))
    this.save()
  }

  setBlend(blend: string): void {
    if (!isBlend(blend)) return
    this.values.blend = blend
    this.save()
  }

  reset(): void {
    this.values = structuredClone(DEFAULT_BAND_STYLE)
    this.save()
  }

  private save(): void {
    setSetting(KEY, JSON.stringify(this.values))
  }
}

export const bandStyle = new BandStyle()
