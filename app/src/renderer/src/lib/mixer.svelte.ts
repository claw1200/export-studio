import { crossfade, faderPosition } from './crossfade'
import { Player } from './player.svelte'
import { getSetting, setSetting } from './settings'

function storedCrossfader(): number {
  const raw = getSetting('crossfader')
  const n = raw == null ? NaN : Number(raw)
  return Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0.5
}

/**
 * Two decks heard through a crossfader. Play starts both in the same instant from wherever their
 * playheads are; stop pauses both and puts each back where it started, ready to hear the same
 * transition again.
 */
export class Mixer {
  /** 0 is all deck A, 1 all deck B. Kept between sessions. */
  crossfader = $state(storedCrossfader())
  /** Where each deck was when play was pressed. Null while stopped. */
  private starts = $state.raw<[number, number] | null>(null)

  constructor(
    readonly a: Player,
    readonly b: Player
  ) {
    this.apply()
  }

  /** Either deck is sounding. A deck that plays to its end stops by itself. */
  get playing(): boolean {
    return this.a.playing || this.b.playing
  }

  /** Something is loaded to play. */
  get ready(): boolean {
    return [this.a, this.b].some((player) => player.track && !player.audioError)
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

  /** A deck's playhead was set by hand: that is where a later stop returns it. */
  placed(player: Player, seconds: number): void {
    const starts = this.starts
    if (!starts) return
    this.starts = player === this.a ? [seconds, starts[1]] : [starts[0], seconds]
  }

  private apply(): void {
    const [a, b] = crossfade(this.crossfader)
    this.a.setGain(a)
    this.b.setGain(b)
  }
}
