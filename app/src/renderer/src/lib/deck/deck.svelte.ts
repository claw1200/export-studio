import type { DeckEvent, DeckTick, Tick } from '../../../../preload/audio'
import { type Anchor, extrapolate, follow, NO_ANCHOR, pinned } from './clock'

/** Longer than this without an animation frame and ticks set the position directly. */
const FRAME_STALL_MS = 100

/** A loop as the engine holds it, in seconds. */
export interface DeckLoop {
  start: number
  end: number
  /** Playback is held inside it. */
  active: boolean
}

/**
 * One deck of the native engine, as reactive state.
 *
 * Commands change this state at once, so the interface responds before the engine replies; ticks
 * then correct it. A tick taken before the engine applied the latest command is ignored, so it
 * cannot pull the playhead back to where it was. Between ticks the playhead is extrapolated.
 */
export class Deck {
  /** The engine has the track decoded and ready. */
  ready = $state(false)
  playing = $state(false)
  scrubbing = $state(false)
  /** Seconds, as drawn. */
  position = $state(0)
  duration = $state(0)
  loop = $state.raw<DeckLoop | null>(null)
  error = $state<string | null>(null)

  private request = 0
  private sent = 0
  private anchor: Anchor = NO_ANCHOR
  private generation = -1
  private raf = 0
  private lastFrame = 0
  /** What to apply once the engine has the track: the playhead may have moved while it loaded. */
  private pending: { seek: boolean } | null = null

  constructor(private index = 0) {
    window.audio.onTick((tick) => this.tick(tick))
    window.audio.onEvent((event) => this.event(event))
    void window.audio.status().then((status) => {
      if ('error' in status) this.error = status.error
    })
  }

  private send(command: Parameters<typeof window.audio.send>[0]): void {
    this.sent = window.audio.send(command)
  }

  load(trackId: string, duration: number): void {
    this.request++
    this.ready = false
    this.playing = false
    this.scrubbing = false
    this.loop = null
    this.error = null
    this.position = 0
    this.duration = duration
    this.anchor = NO_ANCHOR
    this.pending = { seek: false }
    this.stopFrames()
    this.send({ op: 'load', deck: this.index, trackId, request: this.request })
  }

  unload(): void {
    this.request++
    this.ready = false
    this.playing = false
    this.pending = null
    this.stopFrames()
    this.send({ op: 'unload', deck: this.index })
  }

  play(): void {
    if (this.error) return
    this.playing = true
    if (!this.ready) return
    this.anchor = { ...pinned(this.anchor, this.position, performance.now()), playing: !this.scrubbing }
    this.send({ op: 'play', deck: this.index })
    this.startFrames()
  }

  pause(): void {
    this.playing = false
    this.hold(this.position)
    if (this.ready) this.send({ op: 'pause', deck: this.index })
  }

  seek(seconds: number): void {
    this.move(seconds)
    if (this.ready) this.send({ op: 'seek', deck: this.index, ms: seconds * 1000 })
    else if (this.pending) this.pending.seek = true
  }

  /** Pause and move in one step, so the old position is never heard after the stop. */
  stopAt(seconds: number): void {
    this.playing = false
    this.hold(seconds)
    if (this.ready) this.send({ op: 'stopAt', deck: this.index, ms: seconds * 1000 })
    else if (this.pending) this.pending.seek = true
  }

  scrubBegin(): void {
    if (this.scrubbing) return
    this.scrubbing = true
    this.hold(this.position)
    if (this.ready) this.send({ op: 'scrubBegin', deck: this.index })
  }

  /** Moves the scrub target. `at` is the input event's timestamp, which sets how fast the head moves. */
  scrubTo(seconds: number, at: number = performance.now()): void {
    this.hold(seconds)
    if (this.ready) this.send({ op: 'scrubTo', deck: this.index, ms: seconds * 1000, at })
    else if (this.pending) this.pending.seek = true
  }

  scrubEnd(): void {
    if (!this.scrubbing) return
    this.scrubbing = false
    this.anchor = { ...pinned(this.anchor, this.position, performance.now()), playing: this.playing }
    if (this.ready) this.send({ op: 'scrubEnd', deck: this.index })
    if (this.playing) this.startFrames()
  }

  /** Sets a loop and enters it. A playhead outside it goes to the in point. */
  setLoop(start: number, end: number): void {
    if (!(end > start)) return
    this.loop = { start, end, active: true }
    if (this.position < start || this.position >= end) this.move(start)
    else this.anchor = { ...this.anchor, loop: [start, end] }
    if (this.ready) this.send({ op: 'setLoop', deck: this.index, inMs: start * 1000, outMs: end * 1000 })
    else if (this.pending) this.pending.seek = true
  }

  /** RELOOP: back to the in point and inside the loop. EXIT: carry on past it, keeping the range. */
  setLooping(on: boolean): void {
    const loop = this.loop
    if (!loop || loop.active === on) return
    this.loop = { ...loop, active: on }
    if (on) this.move(loop.start)
    else this.anchor = { ...this.anchor, loop: null }
    if (this.ready) this.send({ op: 'setLooping', deck: this.index, on })
    else if (this.pending && on) this.pending.seek = true
  }

  clearLoop(): void {
    if (!this.loop) return
    this.loop = null
    this.anchor = { ...this.anchor, loop: null }
    if (this.ready) this.send({ op: 'clearLoop', deck: this.index })
  }

  setVolume(gain: number): void {
    this.send({ op: 'volume', gain })
  }

  private hold(seconds: number): void {
    this.position = seconds
    this.anchor = pinned(this.anchor, seconds, performance.now())
  }

  /** The playhead jumped and carries on as it was: what the engine does itself on a loop entry. */
  private move(seconds: number): void {
    this.position = seconds
    this.anchor = { ...pinned(this.anchor, seconds, performance.now()), playing: this.playing && !this.scrubbing }
  }

  private activeLoop(): [number, number] | null {
    return this.loop?.active ? [this.loop.start, this.loop.end] : null
  }

  private event(event: DeckEvent): void {
    if (event.deck !== this.index || event.loadId !== this.request) return
    if (event.kind === 'error') {
      this.error = event.message ?? 'The track could not be played'
      this.ready = false
      this.playing = false
      this.pending = null
      return
    }
    this.ready = true
    if (event.sampleRate > 0 && event.totalFrames > 0) this.duration = event.totalFrames / event.sampleRate
    this.anchor = { ...NO_ANCHOR, sampleRate: event.sampleRate, at: performance.now() }
    this.flush()
  }

  /** Replays what happened while the track loaded. */
  private flush(): void {
    const pending = this.pending
    this.pending = null
    if (!pending) return
    const deck = this.index
    if (this.loop) {
      const { start, end, active } = this.loop
      this.send({ op: 'setLoop', deck, inMs: start * 1000, outMs: end * 1000 })
      if (!active) this.send({ op: 'setLooping', deck, on: false })
    }
    if (pending.seek && this.position > 0) this.send({ op: 'seek', deck, ms: this.position * 1000 })
    this.anchor = pinned(this.anchor, this.position, performance.now())
    if (this.scrubbing) this.send({ op: 'scrubBegin', deck })
    if (this.playing) this.play()
  }

  private tick(tick: Tick): void {
    if (tick.seq < this.sent) return
    const deck: DeckTick | undefined = tick.decks[this.index]
    if (!deck || !this.ready || !deck.loaded || deck.loadId !== this.request) return
    const now = performance.now()
    if (this.playing !== deck.playing) this.playing = deck.playing
    if (deck.loopOutFrames > deck.loopInFrames && deck.sampleRate > 0) {
      const start = deck.loopInFrames / deck.sampleRate
      const end = deck.loopOutFrames / deck.sampleRate
      const loop = this.loop
      const moved = !loop || Math.abs(loop.start - start) > 1e-4 || Math.abs(loop.end - end) > 1e-4
      if (moved || loop.active !== deck.looping) this.loop = { start, end, active: deck.looping }
    } else if (this.loop) this.loop = null
    this.anchor = {
      frames: deck.positionFrames,
      at: now,
      sampleRate: deck.sampleRate,
      playing: deck.playing && !deck.scrubbing,
      generation: deck.generation,
      rate: 1,
      loop: this.activeLoop()
    }
    if (this.scrubbing) return
    // A hidden window gets no animation frames, so ticks keep the position true there.
    const framesStalled = now - this.lastFrame > FRAME_STALL_MS
    if (!this.anchor.playing || deck.generation !== this.generation || framesStalled) {
      this.generation = deck.generation
      this.position = extrapolate(this.anchor, now)
    }
    if (this.anchor.playing) this.startFrames()
  }

  private startFrames(): void {
    if (this.raf) return
    this.lastFrame = performance.now()
    this.raf = requestAnimationFrame(this.frame)
  }

  private stopFrames(): void {
    cancelAnimationFrame(this.raf)
    this.raf = 0
  }

  private frame = (now: number): void => {
    this.raf = 0
    if (this.scrubbing || !this.anchor.playing) return
    const since = now - this.lastFrame
    this.lastFrame = now
    const target = extrapolate(this.anchor, now)
    const advance = (Math.max(0, since) / 1000) * this.anchor.rate
    const next = follow(this.position, target, since, advance, this.anchor.loop)
    this.position = this.duration > 0 ? Math.min(next, this.duration) : next
    this.raf = requestAnimationFrame(this.frame)
  }
}
