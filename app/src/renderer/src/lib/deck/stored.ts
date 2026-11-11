import { getSetting, setSetting } from '../settings'

/** Deck preferences kept between sessions. */

export function clampUnit(n: number): number {
  return Math.min(1, Math.max(0, n))
}

export function storedVolume(): number {
  const raw = getSetting('volume')
  if (raw == null) return 1
  const n = Number(raw)
  return Number.isFinite(n) ? clampUnit(n) : 1
}

export function storedFlag(key: string, fallback = false): boolean {
  const value = getSetting(key)
  if (value == null) return fallback
  return value === '1'
}

export function storeFlag(key: string, on: boolean): void {
  setSetting(key, on ? '1' : '0')
}

export function storedNumber(key: string, fallback: number, min: number, max: number): number {
  const raw = getSetting(key)
  if (raw == null) return fallback
  const n = Number(raw)
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : fallback
}

export type Percent = number

export function storedPercent(key: string, fallback: Percent): Percent {
  return Math.round(storedNumber(key, fallback, 0, 100))
}
