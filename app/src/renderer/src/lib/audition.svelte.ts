import { Deck, type DeckLoop } from './deck/deck.svelte'
import type { Cue, Track } from './types'

/**
 * Plays a browser row on its own deck. The loaded player track stays put: a preview pauses it
 * and starts it again when the preview ends. The player taking the speakers stops the preview.
 */
export class Audition {
  trackId = $state<string | null>(null)
  /** Pauses the player as a preview takes the speakers. */
  onBegan: (() => void) | null = null
  /** Starts the player again after a preview that paused it. */
  onEnded: (() => void) | null = null
  private deck = new Deck(1)
  private tookFocus = false

  get position(): number {
    return this.deck.position
  }

  get loop(): DeckLoop | null {
    return this.deck.loop
  }

  get playing(): boolean {
    return this.deck.playing
  }

  /** Hear `track` from `seconds`. A later call on the same track seeks instead of reloading. */
  playAt(track: Track, seconds: number): void {
    this.focus()
    this.ensure(track)
    this.deck.clearLoop()
    this.deck.seek(Math.max(0, seconds))
    this.deck.play()
  }

  /** Hear `track` from a cue. A loop cue plays inside its range. */
  playCue(track: Track, cue: Cue): void {
    this.focus()
    this.ensure(track)
    const start = Math.max(0, cue.start / 1000)
    if (cue.end != null && cue.end > cue.start) this.deck.setLoop(start, cue.end / 1000)
    else {
      this.deck.clearLoop()
      this.deck.seek(start)
    }
    this.deck.play()
  }

  stop(): void {
    if (!this.trackId) return
    this.trackId = null
    this.deck.pause()
    this.deck.unload()
    if (!this.tookFocus) return
    this.tookFocus = false
    this.onEnded?.()
  }

  private focus(): void {
    if (this.tookFocus) return
    this.tookFocus = true
    this.onBegan?.()
  }

  private ensure(track: Track): void {
    if (this.trackId === track.id) return
    this.trackId = track.id
    this.deck.load(track.id, track.duration)
  }
}
