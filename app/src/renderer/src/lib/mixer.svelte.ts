import { untrack } from 'svelte'
import {
  bassLevel,
  bassSwap,
  crossfade,
  FADER_CENTRE,
  faderPosition,
  rebaseBassSwap,
  soleAudible,
  startBassSwap,
  type BassSwap
} from './crossfade'
import type { Seconds } from './deck/clock'
import { Deck } from './deck/deck.svelte'
import type { MixPrefs } from './mix/prefs.svelte'
import { Player } from './player.svelte'
import { getSetting, setSetting } from './settings'

type BeatsForSync = { timesMs: number[]; bpms: number[] }

function beatsForSync(player: Player): BeatsForSync {
  const grid = player.displayGrid
  if (grid?.times.length) return { timesMs: grid.times, bpms: grid.bpms }
  const bpm = player.track?.bpm ?? 0
  return bpm > 0 ? { timesMs: [0], bpms: [bpm] } : { timesMs: [], bpms: [] }
}

function storedCrossfader(): number {
  const raw = getSetting('crossfader')
  const n = raw == null ? NaN : Number(raw)
  return Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : FADER_CENTRE
}

/**
 * The engine, not the mixer, keeps a synced follower on the master's tempo and lands a deck
 * started while the other plays on the other's beat: only it knows where both playheads are at
 * the same instant.
 */
export class Mixer {
  crossfader = $state(storedCrossfader())
  master: Player
  synced = $state(false)
  swapBass = $state(getSetting('swapBass') === 'true')
  lowKnobs = $state.raw<[a: number, b: number]>([1, 1])

  private readonly players: readonly [Player, Player]
  private playBothStartedFrom: [Seconds, Seconds] | null = null
  private swap: BassSwap = startBassSwap(this.crossfader)

  constructor(
    readonly a: Player,
    readonly b: Player,
    readonly prefs: MixPrefs
  ) {
    this.players = [a, b]
    this.master = $state.raw(a)
    this.applyFader()
    this.shareSync()
  }

  get follower(): Player {
    return this.master === this.a ? this.b : this.a
  }

  label(player: Player): 'A' | 'B' {
    return player === this.a ? 'A' : 'B'
  }

  get canSync(): boolean {
    return this.players.every((player) => player.track && player.bpmUnderPlayhead > 0)
  }

  following(player: Player): boolean {
    return this.synced && player === this.follower
  }

  get playing(): boolean {
    return this.players.some((player) => player.playing)
  }

  get playable(): boolean {
    return this.players.some((player) => player.playable)
  }

  setMaster(player: Player): void {
    this.master = player
    this.shareSync()
  }

  /**
   * While both or neither deck is heard the master stays as it is, so a deck picked by hand keeps
   * it. Run it from an effect.
   */
  makeSoleAudibleDeckMaster(): void {
    if (!this.prefs.autoMaster) return
    const playing = this.players.map((player) => player.playing) as [boolean, boolean]
    const sole = soleAudible(playing, crossfade(this.crossfader, this.prefs.curve))
    if (sole == null) return
    const player = this.players[sole]
    untrack(() => {
      if (this.master !== player) this.setMaster(player)
    })
  }

  setSynced(on: boolean): void {
    this.synced = on && this.canSync
    this.shareSync()
  }

  /** Run it from an effect, so the engine hears of every change to the grids. */
  shareBeatgrids(): void {
    for (const player of this.players) {
      const { timesMs, bpms } = beatsForSync(player)
      player.deck.setBeatgrid(timesMs, bpms)
    }
  }

  setSwapBass(on: boolean): void {
    this.swapBass = on
    setSetting('swapBass', String(on))
    this.swap = startBassSwap(this.crossfader, this.prefs.swapRange)
    this.applyFader()
  }

  setCrossfader(position: number): void {
    this.crossfader = faderPosition(position)
    this.applyFader()
  }

  saveCrossfader(): void {
    setSetting('crossfader', String(this.crossfader))
  }

  play(): void {
    if (!this.playable) return
    this.playBothStartedFrom = [this.a.position, this.b.position]
    this.applyFader()
    Player.playTogether(this.players)
  }

  stop(): void {
    const startedFrom = this.playBothStartedFrom
    this.playBothStartedFrom = null
    for (const [i, player] of this.players.entries()) {
      if (startedFrom) player.stopAt(startedFrom[i])
      else player.pause()
    }
  }

  toggle(): void {
    if (this.playing) this.stop()
    else this.play()
  }

  /** A deck started from silence begins a new take, so stop no longer returns to the last play-both. */
  toggleDeck(player: Player): void {
    const startingFromSilence = !this.playing && !player.cueHeld
    if (startingFromSilence) this.playBothStartedFrom = null
    player.toggle()
  }

  /**
   * The swap carries on from where it stands, as though the fade began here, so the bass does not
   * jump. Run it from an effect.
   */
  applyPrefs(): void {
    void this.prefs.curve
    void this.prefs.swapRange
    untrack(() => {
      this.swap = rebaseBassSwap(this.swap.share, this.swap.position)
      this.applyFader()
    })
  }

  private shareSync(): void {
    for (const player of this.players) player.tempo.lockedToMaster = this.following(player)
    Deck.setSync(this.synced ? { master: this.master.deck, follower: this.follower.deck } : null)
  }

  private applyFader(): void {
    const [a, b] = crossfade(this.crossfader, this.prefs.curve)
    this.a.deck.setGain(a)
    this.b.deck.setGain(b)
    if (this.swapBass) {
      this.swap = bassSwap(this.swap, this.crossfader, this.prefs.swapRange)
      this.lowKnobs = [1 - this.swap.share, this.swap.share]
      this.a.deck.setIsolatorLow(bassLevel(this.lowKnobs[0]))
      this.b.deck.setIsolatorLow(bassLevel(this.lowKnobs[1]))
    } else {
      this.lowKnobs = [1, 1]
      this.a.deck.bypassIsolator()
      this.b.deck.bypassIsolator()
    }
  }
}
