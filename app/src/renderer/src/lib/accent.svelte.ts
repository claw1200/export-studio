import { getSetting, setSetting } from './settings'

/** The highlight used until a custom color is saved. */
export const DEFAULT_ACCENT = '#00aaff'

/** Previous default. A saved copy of it is treated as unset so the new default applies. */
const RETIRED_ACCENT = '#00d7e8'

const KEY = 'accentColor'
const HEX = /^#[0-9a-f]{6}$/

function stored(): string {
  const value = getSetting(KEY)
  if (!value) return DEFAULT_ACCENT
  const hex = value.toLowerCase()
  if (!HEX.test(hex) || hex === RETIRED_ACCENT) return DEFAULT_ACCENT
  return hex
}

/** Paints the highlight. The default leaves the stylesheet color in place. */
function paint(color: string): void {
  const root = document.documentElement
  if (color === DEFAULT_ACCENT) {
    root.style.removeProperty('--color-accent')
    root.style.removeProperty('--color-accent-soft')
    return
  }
  root.style.setProperty('--color-accent', color)
  root.style.setProperty('--color-accent-soft', `color-mix(in oklab, ${color} 14%, transparent)`)
}

/** The highlight color, kept between sessions. */
export class Accent {
  color = $state(stored())

  constructor() {
    paint(this.color)
  }

  set(color: string): void {
    const hex = color.toLowerCase()
    if (!HEX.test(hex) || hex === this.color) return
    this.color = hex
    paint(hex)
    setSetting(KEY, hex)
  }

  reset(): void {
    this.set(DEFAULT_ACCENT)
  }
}

export const accent = new Accent()
