import { untrack } from 'svelte'
import { bpmAt } from './beat'
import {
  bassLevel,
  bassSwap,
  crossfade,
  faderPosition,
  rebaseBassSwap,
  soleAudible,
  startBassSwap,
  type BassSwap
} from './crossfade'
import type { MixPrefs } from './mix/prefs.svelte'
import { stepTempo } from './mix/sync'
import { Deck } from './deck/deck.svelte'
import { Player } from './player.svelte'
import { getSetting, setSetting } from './settings'

/** The BPM of the music under the playhead, at the file's own speed. */
export function ownBpm(player: Player): number {
  return bpmAt(player.position, player.displayGrid, player.track?.bpm ?? 0)
}

/** The BPM the deck is playing at: its music's, at its speed. */
export function playingBpm(player: Player): number {
  return ownBpm(player) * player.tempo.rate
}

/** The beats sync goes by: the grid, or without one a beat from the start at the track's BPM. */
function syncGrid(player: Player): { timesMs: number[]; bpms: number[] } {
  const grid = player.displayGrid
  if (grid?.times.length) return { timesMs: grid.times, bpms: grid.bpms }
  const bpm = player.track?.bpm ?? 0
  return bpm > 0 ? { timesMs: [0], bpms: [bpm] } : { timesMs: [], bpms: [] }
}

function storedCrossfader(): number {
  const raw = getSetting('crossfader')
  const n = raw == null ? NaN : Number(raw)
  return Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0.5
}

/**
 * Two decks heard through a crossfader. Play starts both in the same instant from wherever their
 * playheads are; stop pauses both and puts each back where it started, ready to hear the same
 * transition again.
 *
 * One deck is the master. With sync on, the other follows it: the engine keeps its tempo on the
 * master's (at half or double time when that is nearer its own speed), and a deck started while
 * the other plays lands on the other's beat. The engine does both because only it knows where
 * both playheads are at the same instant.
 */
export class Mixer {
  /** 0 is all deck A, 1 all deck B. Kept between sessions. */
  crossfader = $state(storedCrossfader())
  /** Where each deck was when play was pressed. Null while stopped. */
  private starts = $state.raw<[number, number] | null>(null)
  /** The deck the other follows when synced. */
  master: Player
  synced = $state(false)
  /** The fader swaps the bass: the deck faded away from gives its low end to the other. Kept between sessions. */
  swapBass = $state(getSetting('swapBass') === 'true')
  /** Where the swap stands. Not reactive: only `apply` reads it, as the fader moves. */
  private swap: BassSwap = startBassSwap(this.crossfader)
  /** Each deck's LOW knob, 0 fully left (killed) to 1 at the centre (unity), as the swap last set it. */
  lows = $state.raw<[a: number, b: number]>([1, 1])

  constructor(
    readonly a: Player,
    readonly b: Player,
    readonly prefs: MixPrefs
  ) {
    this.master = $state.raw(a)
    this.apply()
    this.shareLink()
  }

  /** The deck that is not the master. */
  get follower(): Player {
    return this.master === this.a ? this.b : this.a
  }

  /** Both decks have a tempo to match. */
  get canSync(): boolean {
    return [this.a, this.b].every((player) => player.track && ownBpm(player) > 0)
  }

  /** `player` is the follower while synced, so its tempo is the master's to set. */
  following(player: Player): boolean {
    return this.synced && player === this.follower
  }

  setMaster(player: Player): void {
    if (player !== this.a && player !== this.b) return
    this.master = player
    this.shareLink()
  }

  /**
   * Makes the deck that alone has sound, playing and not faded out, the master. While both or
   * neither are heard the master stays as it is, so a deck picked by hand keeps it. Run it from an
   * effect: it reads both decks' play state and the fader. Off when `prefs.autoMaster` is.
   */
  followSound(): void {
    if (!this.prefs.autoMaster) return
    const sole = soleAudible([this.a.playing, this.b.playing], crossfade(this.crossfader, this.prefs.curve))
    if (sole == null) return
    const player = sole === 0 ? this.a : this.b
    untrack(() => {
      if (this.master !== player) this.setMaster(player)
    })
  }

  /**
   * Turning sync on matches the follower's tempo to the master's and snaps it onto the master's
   * nearest beat when both are playing or both paused: it moves by under half a beat, so it stays
   * where it was in its track to within a beat.
   */
  setSynced(on: boolean): void {
    this.synced = on && this.canSync
    this.shareLink()
  }

  private shareLink(): void {
    Deck.setSync(this.synced ? { master: this.master.deck, follower: this.follower.deck } : null)
  }

  /** Gives the engine each deck's beats. Run it from an effect: it reads both grids. */
  shareBeatgrids(): void {
    for (const player of [this.a, this.b]) {
      const { timesMs, bpms } = syncGrid(player)
      player.deck.setBeatgrid(timesMs, bpms)
    }
  }

  /** Moves a deck's tempo by `step` BPM. The follower's is the master's while synced. */
  stepBpm(player: Player, step: number): void {
    if (this.following(player)) return
    const rate = stepTempo(playingBpm(player), ownBpm(player), step)
    if (rate != null) player.tempo.setRate(rate)
  }

  /** Back to the file's own speed. */
  resetTempo(player: Player): void {
    if (this.following(player)) return
    player.tempo.setRate(1)
  }

  stepKey(player: Player, step: number): void {
    player.tempo.setKeyShift(player.tempo.keyShift + step)
  }

  resetKey(player: Player): void {
    player.tempo.setKeyShift(0)
  }

  /** Master tempo on a mix deck is its own: it does not change the default for other decks. */
  toggleMasterTempo(player: Player): void {
    player.tempo.setMasterTempo(!player.tempo.masterTempo, false)
  }

  /** Either deck is sounding. A deck that plays to its end stops by itself. */
  get playing(): boolean {
    return this.a.playing || this.b.playing
  }

  /** Something is loaded to play. */
  get ready(): boolean {
    return [this.a, this.b].some((player) => player.track && !player.audioError)
  }

  setSwapBass(on: boolean): void {
    this.swapBass = on
    setSetting('swapBass', String(on))
    this.swap = startBassSwap(this.crossfader, this.prefs.swapRange)
    this.apply()
  }

  setCrossfader(position: number): void {
    this.crossfader = faderPosition(position)
    this.apply()
  }

  /** Saves the position: called when a drag ends rather than on every move. */
  commitCrossfader(): void {
    setSetting('crossfader', String(this.crossfader))
  }

  play(): void {
    if (!this.ready) return
    this.starts = [this.a.position, this.b.position]
    this.apply()
    // `playTogether` skips an empty deck, so one track alone plays too. The engine lines a synced
    // follower up first.
    Player.playTogether([this.a, this.b])
  }

  stop(): void {
    const starts = this.starts
    this.starts = null
    for (const [i, player] of [this.a, this.b].entries()) {
      if (starts) player.stopAt(starts[i])
      else player.pause()
    }
  }

  toggle(): void {
    if (this.playing) this.stop()
    else this.play()
  }

  /**
   * One deck's own play/pause, leaving the other as it is. While synced, the engine starts it on
   * the other deck's beat, so it can be dropped in over the other. Starting a deck while both are
   * stopped forgets where the last play-both began, so stop does not jump back to it.
   */
  toggleDeck(player: Player): void {
    if (player !== this.a && player !== this.b) return
    if (!player.cueHeld && !this.playing) this.starts = null
    player.toggle()
  }

  /**
   * Picks up a change to the curve or swap range. The swap carries on from where it stands, as
   * though the fade began here, so the bass does not jump. Run it from an effect: it reads `prefs`.
   */
  retune(): void {
    void this.prefs.curve
    void this.prefs.swapRange
    untrack(() => {
      this.swap = rebaseBassSwap(this.swap.share, this.swap.position)
      this.apply()
    })
  }

  private apply(): void {
    const [a, b] = crossfade(this.crossfader, this.prefs.curve)
    this.a.setGain(a)
    this.b.setGain(b)
    if (this.swapBass) {
      this.swap = bassSwap(this.swap, this.crossfader, this.prefs.swapRange)
      this.lows = [1 - this.swap.share, this.swap.share]
      this.a.setIsolatorLow(bassLevel(this.lows[0]))
      this.b.setIsolatorLow(bassLevel(this.lows[1]))
    } else {
      this.lows = [1, 1]
      this.a.bypassIsolator()
      this.b.bypassIsolator()
    }
  }
}
