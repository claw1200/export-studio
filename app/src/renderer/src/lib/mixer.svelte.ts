import { untrack } from 'svelte'
import { bpmAt, fractionalBeatIndex } from './beat'
import {
  bassLevel,
  bassSwap,
  crossfade,
  faderPosition,
  rebaseBassSwap,
  startBassSwap,
  type BassSwap
} from './crossfade'
import type { MixPrefs } from './mix/prefs.svelte'
import { beatOffset, matchTempo, stepTempo } from './mix/sync'
import { Player } from './player.svelte'
import { getSetting, setSetting } from './settings'

/** A follower this close to the master's beat, in seconds, is left where it is. */
const ALIGNED_SECONDS = 0.001

/** The BPM of the music under the playhead, at the file's own speed. */
export function ownBpm(player: Player): number {
  return bpmAt(player.position, player.displayGrid, player.track?.bpm ?? 0)
}

/** The BPM the deck is playing at: its music's, at its speed. */
export function playingBpm(player: Player): number {
  return ownBpm(player) * player.tempo.rate
}

/** Where the playhead is in beats: along the grid, or counted from the start at the track's BPM. */
function beatAt(player: Player): number | null {
  const grid = player.displayGrid
  if (grid?.times.length) return fractionalBeatIndex(grid, player.position * 1000)
  const bpm = ownBpm(player)
  return bpm > 0 ? (player.position * bpm) / 60 : null
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
 * One deck is the master. With sync on, the other follows it: its tempo is kept on the master's
 * (at half or double time when that is nearer its own speed), and play lines its beats up with
 * the master's before starting.
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
    this.follow()
  }

  /**
   * Turning sync on matches the follower's tempo to the master's and snaps it onto the master's
   * nearest beat, playing or not: it moves by under half a beat, so it stays where it was in its
   * track to within a beat.
   */
  setSynced(on: boolean): void {
    this.synced = on && this.canSync
    if (!this.synced) return
    this.follow()
    this.alignBeats()
  }

  /**
   * Keeps the follower's tempo on the master's. Run it from an effect: it reads both playheads, so
   * a tempo change along either beatgrid is followed as the decks play through it.
   */
  follow(): void {
    if (!this.synced) return
    const follower = this.follower
    const match = matchTempo(playingBpm(this.master), ownBpm(follower))
    if (!match) return
    const rate = untrack(() => follower.tempo.rate)
    if (Math.abs(rate - match.rate) > 1e-6) follower.tempo.setRate(match.rate)
  }

  /**
   * Moves the follower by under half a beat so its beats land on the master's. It is a nudge from
   * wherever the engine's playhead is, so the time the command takes to arrive does not count,
   * and while playing the engine keeps time through it.
   */
  private alignBeats(): void {
    const master = this.master
    const follower = this.follower
    const match = matchTempo(playingBpm(master), ownBpm(follower))
    const masterBeat = beatAt(master)
    const ownBeat = beatAt(follower)
    const bpm = ownBpm(follower)
    if (!match || masterBeat == null || ownBeat == null || !(bpm > 0)) return
    const seconds = (beatOffset(masterBeat, ownBeat, match.beatsPerMasterBeat) * 60) / bpm
    if (Math.abs(seconds) >= ALIGNED_SECONDS) follower.nudge(-seconds)
  }

  /** Moves a deck's tempo by `step` BPM. The follower's is the master's while synced. */
  stepBpm(player: Player, step: number): void {
    if (this.following(player)) return
    const rate = stepTempo(playingBpm(player), ownBpm(player), step)
    if (rate != null) player.tempo.setRate(rate)
    this.follow()
  }

  /** Back to the file's own speed. */
  resetTempo(player: Player): void {
    if (this.following(player)) return
    player.tempo.setRate(1)
    this.follow()
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
    this.swap = startBassSwap(this.crossfader)
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
    if (this.synced) {
      this.follow()
      this.alignBeats()
    }
    // `playTogether` skips an empty deck, so one track alone plays too.
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
   * One deck's own play/pause, leaving the other as it is. A synced follower lines its beats up
   * with the master as it starts, so it can be dropped in over a playing master. Starting a deck
   * while both are stopped forgets where the last play-both began, so stop does not jump back to it.
   */
  toggleDeck(player: Player): void {
    if (player !== this.a && player !== this.b) return
    // Play while a cue is held latches the hold, as on a CDJ.
    if (player.cueHeld || player.playing) {
      player.toggle()
      return
    }
    if (!this.playing) this.starts = null
    if (this.following(player)) {
      this.follow()
      this.alignBeats()
    }
    player.play()
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
      this.a.setLow(bassLevel(this.lows[0]))
      this.b.setLow(bassLevel(this.lows[1]))
    } else {
      this.lows = [1, 1]
      this.a.setLow(null)
      this.b.setLow(null)
    }
  }
}
