import { MAX_TEMPO, MIN_TEMPO } from '../../../../shared/engine'
import { getSetting, setSetting } from '../settings'
import type { Deck } from './deck.svelte'
import { storedFlag, storeFlag } from './stored'

/** How far the fader reaches either side of the file's speed, in percent, as on a CDJ. */
const TEMPO_RANGES = [6, 10, 16, 50] as const
type TempoRange = (typeof TEMPO_RANGES)[number]

/** Smallest tempo change, in percent. */
const TEMPO_STEP = 0.01

function storedRange(): TempoRange {
  const n = Number(getSetting('tempoRange'))
  return TEMPO_RANGES.find((r) => r === n) ?? 10
}

/** The tempo fader: speed in percent within a range, and master tempo to keep the key. */
export class TempoControl {
  range = $state<TempoRange>(storedRange())
  lockedToMaster = $state(false)

  constructor(private deck: Deck) {
    if (storedFlag('masterTempo')) deck.setMasterTempo(true)
  }

  /** Playback speed as a multiple of the file's. */
  get rate(): number {
    return this.deck.tempo
  }

  get percent(): number {
    return (this.deck.tempo - 1) * 100
  }

  get masterTempo(): boolean {
    return this.deck.masterTempo
  }

  setPercent(percent: number): void {
    if (this.lockedToMaster) return
    const inRange = Math.max(-this.range, Math.min(this.range, percent))
    const stepped = Math.round(inRange / TEMPO_STEP) * TEMPO_STEP
    this.deck.setTempo(1 + stepped / 100)
  }

  reset(): void {
    if (this.lockedToMaster) return
    this.deck.setTempo(1)
  }

  setMasterTempo(on: boolean): void {
    this.deck.setMasterTempo(on)
  }

  setMasterTempoAndDefault(on: boolean): void {
    this.setMasterTempo(on)
    storeFlag('masterTempo', on)
  }

  setRate(rate: number): void {
    if (this.lockedToMaster || !(rate > 0)) return
    this.deck.setTempo(Math.max(MIN_TEMPO, Math.min(MAX_TEMPO, rate)))
  }

  /** Semitones the key is moved by, on top of what the tempo does to it. */
  get keyShift(): number {
    return this.deck.keyShift
  }

  setKeyShift(semitones: number): void {
    this.deck.setKeyShift(semitones)
  }

  /** A narrower range pulls the tempo in to fit it, as moving the range switch does. */
  setRange(range: TempoRange): void {
    this.range = range
    setSetting('tempoRange', String(range))
    this.setPercent(this.percent)
  }

  cycleRange(): void {
    const next = TEMPO_RANGES[(TEMPO_RANGES.indexOf(this.range) + 1) % TEMPO_RANGES.length]
    this.setRange(next)
  }
}

/** A tempo as a signed percentage: `+2.35%`, `−0.50%`, `0.00%`. */
export function formatTempo(percent: number): string {
  const rounded = Math.round(percent / TEMPO_STEP) * TEMPO_STEP
  if (Math.abs(rounded) < TEMPO_STEP / 2) return '0.00%'
  return `${rounded > 0 ? '+' : '−'}${Math.abs(rounded).toFixed(2)}%`
}
