import { MAX_TEMPO, MIN_TEMPO } from '../../../../shared/engine'
import { getSetting, setSetting } from '../settings'
import type { Deck } from './deck.svelte'
import { storedFlag, storeFlag } from './stored'

const FILE_SPEED = 1
const TEMPO_RANGES_PERCENT = [6, 10, 16, 50] as const
type TempoRange = (typeof TEMPO_RANGES_PERCENT)[number]
const DEFAULT_TEMPO_RANGE: TempoRange = 10
const TEMPO_STEP_PERCENT = 0.01

function storedRange(): TempoRange {
  const n = Number(getSetting('tempoRange'))
  return TEMPO_RANGES_PERCENT.find((r) => r === n) ?? DEFAULT_TEMPO_RANGE
}

export class TempoControl {
  range = $state<TempoRange>(storedRange())
  lockedToMaster = $state(false)

  constructor(private deck: Deck) {
    if (storedFlag('masterTempo')) deck.setMasterTempo(true)
  }

  get rate(): number {
    return this.deck.tempo
  }

  get percent(): number {
    return (this.deck.tempo - FILE_SPEED) * 100
  }

  get masterTempo(): boolean {
    return this.deck.masterTempo
  }

  setPercent(percent: number): void {
    if (this.lockedToMaster) return
    const inRange = Math.max(-this.range, Math.min(this.range, percent))
    const stepped = Math.round(inRange / TEMPO_STEP_PERCENT) * TEMPO_STEP_PERCENT
    this.deck.setTempo(FILE_SPEED + stepped / 100)
  }

  reset(): void {
    if (this.lockedToMaster) return
    this.deck.setTempo(FILE_SPEED)
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

  get keyShift(): number {
    return this.deck.keyShift
  }

  setKeyShift(semitones: number): void {
    this.deck.setKeyShift(semitones)
  }

  setRange(range: TempoRange): void {
    this.range = range
    setSetting('tempoRange', String(range))
    this.fitTempoInRange()
  }

  private fitTempoInRange(): void {
    this.setPercent(this.percent)
  }

  cycleRange(): void {
    const ranges = TEMPO_RANGES_PERCENT
    this.setRange(ranges[(ranges.indexOf(this.range) + 1) % ranges.length])
  }
}

export function formatTempo(percent: number): string {
  const rounded = Math.round(percent / TEMPO_STEP_PERCENT) * TEMPO_STEP_PERCENT
  if (Math.abs(rounded) < TEMPO_STEP_PERCENT / 2) return '0.00%'
  return `${rounded > 0 ? '+' : '−'}${Math.abs(rounded).toFixed(2)}%`
}
