import { pitchSemitones, shiftKey } from '../format'

/**
 * How well a track's key mixes with the master deck's, best first. Each is a move around the
 * Camelot wheel from the master's key:
 * - `same`: the same key (1A to 1A)
 * - `relative`: the relative major or minor (1A to 1B)
 * - `adjacent`: one step either way, same letter (1A to 2A or 12A)
 * - `two`: two steps up, same letter (1A to 3A, but not 3A to 1A)
 * - `boost`: seven steps up, a semitone higher (1A to 8A)
 */
export const KEY_MATCHES = ['same', 'relative', 'adjacent', 'two', 'boost'] as const
export type KeyMatch = (typeof KEY_MATCHES)[number]

type Camelot = { n: number; minor: boolean }

const CAMELOT = /^(\d{1,2})([AB])$/i
const NOTE = /^(C#|Db|D#|Eb|F#|Gb|G#|Ab|A#|Bb|[A-G])(m)?$/
// prettier-ignore
const PITCH: Record<string, number> = {
  C: 0, 'C#': 1, Db: 1, D: 2, 'D#': 3, Eb: 3, E: 4, F: 5,
  'F#': 6, Gb: 6, G: 7, 'G#': 8, Ab: 8, A: 9, 'A#': 10, Bb: 10, B: 11
}

function mod(n: number, m: number): number {
  return ((n % m) + m) % m
}

/** A key as a place on the Camelot wheel. Takes Camelot (`8A`) or a note name (`Am`, `C`). */
export function toCamelot(key: string): Camelot | null {
  const text = key.trim()
  const camelot = CAMELOT.exec(text)
  if (camelot) {
    const n = Number(camelot[1])
    return n >= 1 && n <= 12 ? { n, minor: camelot[2].toUpperCase() === 'A' } : null
  }
  const note = NOTE.exec(text)
  if (!note) return null
  const minor = note[2] === 'm'
  // C major is 8B and the wheel moves in fifths; a minor key sits with its relative major.
  const major = PITCH[note[1]] + (minor ? 3 : 0)
  return { n: mod(7 * major + 7, 12) + 1, minor }
}

/** How well `key` mixes with `master`, or null when the move isn't one of `KEY_MATCHES`. */
export function keyMatch(master: string, key: string): KeyMatch | null {
  const from = toCamelot(master)
  const to = toCamelot(key)
  if (!from || !to) return null
  const steps = mod(to.n - from.n, 12)
  if (from.minor !== to.minor) return steps === 0 ? 'relative' : null
  if (steps === 0) return 'same'
  if (steps === 1 || steps === 11) return 'adjacent'
  if (steps === 2) return 'two'
  if (steps === 7) return 'boost'
  return null
}

/** `match` is no worse than `cutoff`, the loosest match still highlighted. */
export function withinCutoff(match: KeyMatch | null, cutoff: KeyMatch | null): boolean {
  if (!match || !cutoff) return false
  return KEY_MATCHES.indexOf(match) <= KEY_MATCHES.indexOf(cutoff)
}

/**
 * Highlight hue for each match: the good mixes (same, relative, one step) stay green, and the
 * riskier ones turn towards orange.
 */
const MATCH_HUES: Record<KeyMatch, number> = { same: 150, relative: 140, adjacent: 128, two: 85, boost: 55 }

/** A highlight from green for the same key to orange for the loosest match. */
export function keyMatchColor(match: KeyMatch): string {
  return `oklch(0.78 0.15 ${MATCH_HUES[match]})`
}

/** The key a deck is heard in: its key shift, plus the tempo's pitch change unless master tempo holds it. */
export function heardKey(key: string, tempo: { rate: number; keyShift: number; masterTempo: boolean }): string {
  if (!key) return ''
  return shiftKey(key, tempo.keyShift + (tempo.masterTempo ? 0 : pitchSemitones(tempo.rate)))
}
