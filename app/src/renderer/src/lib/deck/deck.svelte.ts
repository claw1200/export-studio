import type { DeckEvent, DeckTick, Tick } from '../../../../preload/audio'
import { MAX_KEY_SHIFT } from '../../../../shared/engine'
import { type Anchor, easeTowards, extrapolate, type LoopRange, NO_ANCHOR, reanchored, type Seconds } from './clock'
import type { DeckPrefs } from './prefs.svelte'

type EngineCommand = Parameters<typeof window.audio.send>[0]

const FRAME_STALL_MS = 100
const LOOP_EDGE_TOLERANCE: Seconds = 1e-4

export interface DeckLoop {
  start: Seconds
  end: Seconds
  active: boolean
}

function toMs(seconds: Seconds): number {
  return seconds * 1000
}

export class Deck {
  ready = $state(false)
  playing = $state(false)
  scrubbing = $state(false)
  position = $state<Seconds>(0)
  duration = $state<Seconds>(0)
  loop = $state.raw<DeckLoop | null>(null)
  error = $state<string | null>(null)
  tempo = $state(1)
  masterTempo = $state(false)
  keyShift = $state(0)
  onLoadSettled: ((error: string | null) => void) | null = null

  private request = 0
  private lastCommandSeq = 0
  private anchor: Anchor = NO_ANCHOR
  private generation = -1
  private raf = 0
  private lastFrame = 0
  private afterLoad: { seek: boolean } | null = null

  constructor(
    private index = 0,
    private readonly prefs: Pick<DeckPrefs, 'snapPlayhead'> = { snapPlayhead: false }
  ) {
    window.audio.onTick((tick) => this.tick(tick))
    window.audio.onEvent((event) => this.event(event))
    void window.audio.status().then((status) => {
      if ('error' in status) this.error = status.error
    })
  }

  load(trackId: string, duration: Seconds): void {
    this.request++
    this.ready = false
    this.playing = false
    this.scrubbing = false
    this.loop = null
    this.error = null
    this.position = 0
    this.duration = duration
    this.anchor = NO_ANCHOR
    this.afterLoad = { seek: false }
    this.stopFrames()
    this.send({ op: 'load', deck: this.index, trackId, request: this.request })
  }

  unload(): void {
    this.request++
    this.ready = false
    this.playing = false
    this.afterLoad = null
    this.stopFrames()
    this.send({ op: 'unload', deck: this.index })
  }

  play({ onMasterBeat = true }: { onMasterBeat?: boolean } = {}): void {
    this.markPlaying()
    if (!this.canStartNow) return
    this.send({ op: 'play', deck: this.index, unsynced: !onMasterBeat })
    this.startFrames()
  }

  static playTogether(decks: Deck[]): void {
    for (const deck of decks) deck.markPlaying()
    const startable = decks.filter((deck) => deck.canStartNow)
    if (!startable.length) return
    const seq = window.audio.send({ op: 'playTogether', decks: startable.map((deck) => deck.index) })
    for (const deck of startable) {
      deck.lastCommandSeq = seq
      deck.startFrames()
    }
  }

  static setSync(link: { master: Deck; follower: Deck } | null): void {
    const decks = link && { master: link.master.index, follower: link.follower.index }
    window.audio.send({ op: 'sync', link: decks })
  }

  align(): void {
    this.sendIfReady({ op: 'align', deck: this.index })
  }

  setBeatgrid(timesMs: number[], bpms: number[]): void {
    this.send({ op: 'beatgrid', deck: this.index, timesMs, bpms })
  }

  setGain(linearGain: number): void {
    this.send({ op: 'deckGain', deck: this.index, gain: linearGain })
  }

  setIsolatorLow(linearGain: number): void {
    this.send({ op: 'deckLow', deck: this.index, gain: linearGain })
  }

  bypassIsolator(): void {
    this.send({ op: 'deckLow', deck: this.index, gain: null })
  }

  pause(): void {
    this.playing = false
    this.holdAt(this.position)
    this.sendIfReady({ op: 'pause', deck: this.index })
  }

  seek(seconds: Seconds): void {
    this.jumpTo(seconds)
    this.sendOrReplayAfterLoad({ op: 'seek', deck: this.index, ms: toMs(seconds) })
  }

  nudge(offset: Seconds): void {
    if (!Number.isFinite(offset) || offset === 0) return
    this.jumpTo(this.withinTrack(this.position + offset))
    this.sendOrReplayAfterLoad({ op: 'nudge', deck: this.index, ms: toMs(offset) })
  }

  stopAt(seconds: Seconds): void {
    this.playing = false
    this.holdAt(seconds)
    this.sendOrReplayAfterLoad({ op: 'stopAt', deck: this.index, ms: toMs(seconds) })
  }

  scrubBegin(): void {
    if (this.scrubbing) return
    this.scrubbing = true
    this.holdAt(this.position)
    this.sendIfReady({ op: 'scrubBegin', deck: this.index })
  }

  scrubTo(seconds: Seconds, inputTimestamp = performance.now()): void {
    this.holdAt(seconds)
    this.sendOrReplayAfterLoad({ op: 'scrubTo', deck: this.index, ms: toMs(seconds), at: inputTimestamp })
  }

  scrubEnd(): void {
    if (!this.scrubbing) return
    this.scrubbing = false
    this.reanchor(this.position, { playing: this.playheadAdvancing })
    this.sendIfReady({ op: 'scrubEnd', deck: this.index })
    if (this.playing) this.startFrames()
  }

  setLoop(start: Seconds, end: Seconds, enter = true): void {
    if (!(end > start)) return
    this.loop = { start, end, active: enter }
    const playheadOutside = this.position < start || this.position >= end
    if (!enter) this.updateAnchor({ loop: null })
    else if (playheadOutside) this.jumpTo(start)
    else this.updateAnchor({ loop: [start, end] })
    this.sendOrReplayAfterLoad(this.loopCommand(this.loop), enter)
  }

  setLooping(on: boolean): void {
    const loop = this.loop
    if (!loop || loop.active === on) return
    this.loop = { ...loop, active: on }
    if (on) this.jumpTo(loop.start)
    else this.updateAnchor({ loop: null })
    this.sendOrReplayAfterLoad({ op: 'setLooping', deck: this.index, on }, on)
  }

  clearLoop(): void {
    if (!this.loop) return
    this.loop = null
    this.updateAnchor({ loop: null })
    this.sendIfReady({ op: 'clearLoop', deck: this.index })
  }

  setTempo(tempo: number): void {
    if (!(tempo > 0) || tempo === this.tempo) return
    this.tempo = tempo
    this.reanchor(this.position, { rate: tempo })
    this.send({ op: 'tempo', deck: this.index, tempo })
  }

  setMasterTempo(on: boolean): void {
    this.masterTempo = on
    this.send({ op: 'masterTempo', deck: this.index, on })
  }

  setKeyShift(semitones: number): void {
    const shift = Math.max(-MAX_KEY_SHIFT, Math.min(MAX_KEY_SHIFT, Math.round(semitones)))
    if (shift === this.keyShift) return
    this.keyShift = shift
    this.send({ op: 'keyShift', deck: this.index, semitones: shift })
  }

  private send(command: EngineCommand): void {
    this.lastCommandSeq = window.audio.send(command)
  }

  private sendIfReady(command: EngineCommand): void {
    if (this.ready) this.send(command)
  }

  private sendOrReplayAfterLoad(command: EngineCommand, movesPlayhead = true): void {
    if (this.ready) this.send(command)
    else if (movesPlayhead && this.afterLoad) this.afterLoad.seek = true
  }

  private loopCommand({ start, end, active }: DeckLoop): EngineCommand {
    return { op: 'setLoop', deck: this.index, inMs: toMs(start), outMs: toMs(end), enter: active }
  }

  private markPlaying(): void {
    if (this.error) return
    this.playing = true
    if (this.ready) this.reanchor(this.position, { playing: this.playheadAdvancing })
  }

  private get canStartNow(): boolean {
    return this.ready && !this.error
  }

  private get playheadAdvancing(): boolean {
    return this.playing && !this.scrubbing
  }

  private withinTrack(seconds: Seconds): Seconds {
    return Math.max(0, Math.min(seconds, this.duration || Infinity))
  }

  private holdAt(seconds: Seconds): void {
    this.position = seconds
    this.reanchor(seconds, { playing: false })
  }

  private jumpTo(seconds: Seconds): void {
    this.position = seconds
    this.reanchor(seconds, { playing: this.playheadAdvancing })
  }

  private reanchor(seconds: Seconds, changes: Partial<Anchor> = {}): void {
    this.anchor = { ...reanchored(this.anchor, seconds, performance.now()), ...changes }
  }

  private updateAnchor(changes: Partial<Anchor>): void {
    this.anchor = { ...this.anchor, ...changes }
  }

  private activeLoop(): LoopRange | null {
    return this.loop?.active ? [this.loop.start, this.loop.end] : null
  }

  private event(event: DeckEvent): void {
    if (event.deck !== this.index || event.loadId !== this.request) return
    if (event.kind === 'error') this.loadFailed(event.message ?? 'The track could not be played')
    else this.loaded(event)
  }

  private loadFailed(message: string): void {
    this.error = message
    this.ready = false
    this.playing = false
    this.afterLoad = null
    this.onLoadSettled?.(message)
  }

  private loaded({ sampleRate, totalFrames }: DeckEvent): void {
    this.ready = true
    if (sampleRate > 0 && totalFrames > 0) this.duration = totalFrames / sampleRate
    this.anchor = { ...NO_ANCHOR, sampleRate, at: performance.now(), rate: this.tempo }
    this.applyAfterLoad()
    this.onLoadSettled?.(null)
  }

  private applyAfterLoad(): void {
    const afterLoad = this.afterLoad
    this.afterLoad = null
    if (!afterLoad) return
    if (this.loop) this.send(this.loopCommand(this.loop))
    if (afterLoad.seek && this.position > 0) this.send({ op: 'seek', deck: this.index, ms: toMs(this.position) })
    this.holdAt(this.position)
    if (this.scrubbing) this.send({ op: 'scrubBegin', deck: this.index })
    if (this.playing) this.play()
  }

  private tick(tick: Tick): void {
    const predatesLastCommand = tick.seq < this.lastCommandSeq
    if (predatesLastCommand) return
    const deck: DeckTick | undefined = tick.decks[this.index]
    if (!deck || !this.ready || !deck.loaded || deck.loadId !== this.request) return
    const now = performance.now()
    this.playing = deck.playing
    this.tempo = deck.tempo
    this.masterTempo = deck.masterTempo
    this.followEngineLoop(deck)
    this.anchor = {
      frames: deck.positionFrames,
      at: now,
      sampleRate: deck.sampleRate,
      playing: deck.playing && !deck.scrubbing,
      generation: deck.generation,
      rate: deck.tempo,
      loop: this.activeLoop()
    }
    if (this.scrubbing) return
    const seekedOrLoaded = deck.generation !== this.generation
    const framesStalled = now - this.lastFrame > FRAME_STALL_MS
    if (!this.anchor.playing || seekedOrLoaded || framesStalled) {
      this.generation = deck.generation
      this.position = extrapolate(this.anchor, now)
    }
    if (this.anchor.playing) this.startFrames()
  }

  private followEngineLoop(deck: DeckTick): void {
    const engineHasLoop = deck.loopOutFrames > deck.loopInFrames && deck.sampleRate > 0
    if (!engineHasLoop) {
      this.loop = null
      return
    }
    const start = deck.loopInFrames / deck.sampleRate
    const end = deck.loopOutFrames / deck.sampleRate
    const loop = this.loop
    const moved =
      !loop || Math.abs(loop.start - start) > LOOP_EDGE_TOLERANCE || Math.abs(loop.end - end) > LOOP_EDGE_TOLERANCE
    if (moved || loop.active !== deck.looping) this.loop = { start, end, active: deck.looping }
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
    const sinceLastFrame = now - this.lastFrame
    this.lastFrame = now
    const target = extrapolate(this.anchor, now)
    const next = this.prefs.snapPlayhead
      ? target
      : easeTowards(this.position, target, sinceLastFrame, this.anchor.rate, this.anchor.loop)
    this.position = this.duration > 0 ? Math.min(next, this.duration) : next
    this.raf = requestAnimationFrame(this.frame)
  }
}
