import { BrowserWindow, ipcMain } from 'electron'
import type { AudioStatus, DeckCommand, DeckEvent, DeckTick, Tick, WaveAnalysis } from '../../preload/audio'
import type { Engine } from '../engine'
import { loadNative, type NativeEngine, type NativeModule } from './native'

/** Decks the engine runs. The interface has one; the engine is built for more. */
const DECKS = 1

/** Position ticks per second while a deck is sounding. The renderer extrapolates between them. */
const TICK_MS = 33

/** Ticks sent after the last sound, so the interface sees the final position and state. */
const TAIL_TICKS = 5

/**
 * Owns the native engine and connects it to every window: commands in on `deck:cmd`, positions
 * out on `deck:tick`, load results and errors on `deck:event`.
 */
export class AudioBridge {
  private native: NativeModule | null = null
  private engine: NativeEngine | null = null
  private failure: string | null = null
  private ticker: ReturnType<typeof setInterval> | null = null
  private idleTicks = 0
  private tickQueued = false
  /** The load each deck most recently asked for, so a slow path lookup cannot load a stale track. */
  private latestLoad = new Map<number, number>()
  private volume = 1
  /** The newest command applied, echoed in every tick. */
  private seq = 0

  constructor(private library: Engine) {
    try {
      this.native = loadNative()
    } catch (err) {
      this.failure = `The audio engine could not be loaded: ${(err as Error).message}`
      console.error('[audio]', this.failure)
    }
  }

  register(): void {
    ipcMain.handle('audio:status', (): AudioStatus => {
      if (!this.failure) this.open()
      return this.failure ? { available: false, error: this.failure } : { available: true }
    })
    ipcMain.on('deck:cmd', (_e, command: DeckCommand, seq: number) => {
      this.command(command)
      this.seq = Math.max(this.seq, seq)
      this.queueTick()
    })
    ipcMain.handle('wave:analyse', (_e, trackId: string, bands: boolean) => this.analyse(trackId, bands))
  }

  stop(): void {
    if (this.ticker) clearInterval(this.ticker)
    this.ticker = null
    this.engine = null
  }

  /** The engine, opening the output device on first use rather than at launch. */
  private open(): NativeEngine | null {
    if (this.engine || this.failure || !this.native) return this.engine
    try {
      this.engine = new this.native.AudioEngine(DECKS, (event) => this.broadcast('deck:event', event as DeckEvent))
      this.engine.setVolume(this.volume)
    } catch (err) {
      this.failure = `No audio output: ${(err as Error).message}`
      console.error('[audio]', this.failure)
    }
    return this.engine
  }

  private command(command: DeckCommand): void {
    if (command.op === 'volume') this.volume = command.gain
    const engine = this.open()
    if (!engine) return
    switch (command.op) {
      case 'load':
        this.latestLoad.set(command.deck, command.request)
        void this.load(engine, command.deck, command.trackId, command.request)
        break
      case 'unload':
        this.latestLoad.delete(command.deck)
        engine.unload(command.deck)
        break
      case 'play':
        engine.play(command.deck)
        break
      case 'pause':
        engine.pause(command.deck)
        break
      case 'seek':
        engine.seek(command.deck, command.ms)
        break
      case 'stopAt':
        engine.stopAt(command.deck, command.ms)
        break
      case 'scrubBegin':
        engine.scrubBegin(command.deck)
        break
      case 'scrubTo':
        engine.scrubTo(command.deck, command.ms, command.at)
        break
      case 'scrubEnd':
        engine.scrubEnd(command.deck)
        break
      case 'setLoop':
        engine.setLoop(command.deck, command.inMs, command.outMs, command.enter)
        break
      case 'setLooping':
        engine.setLooping(command.deck, command.on)
        break
      case 'clearLoop':
        engine.clearLoop(command.deck)
        break
      case 'volume':
        engine.setVolume(command.gain)
        break
    }
    this.startTicker()
  }

  private async resolvePath(trackId: string): Promise<string | null> {
    return this.library.call<string | null>('media.resolve', { kind: 'audio', id: trackId, size: '' })
  }

  private async load(engine: NativeEngine, deck: number, trackId: string, request: number): Promise<void> {
    let path: string | null = null
    try {
      path = await this.resolvePath(trackId)
    } catch (err) {
      console.error('[audio] could not resolve', trackId, err)
    }
    if (this.latestLoad.get(deck) !== request) return
    if (!path) {
      engine.unload(deck)
      this.broadcast('deck:event', {
        kind: 'error',
        deck,
        loadId: request,
        totalFrames: 0,
        sampleRate: 0,
        message: 'Audio file not found'
      } satisfies DeckEvent)
      return
    }
    engine.load(deck, path, request)
    this.queueTick()
  }

  private async analyse(trackId: string, bands: boolean): Promise<WaveAnalysis | null> {
    if (!this.native) return null
    const path = await this.resolvePath(trackId).catch(() => null)
    if (!path) return null
    try {
      const wave = await this.native.analyse(path, bands)
      if (!wave) return null
      return {
        rate: wave.rate,
        data: wave.data,
        peak: wave.peak,
        ...(wave.bands ? { bands: wave.bands, bandRate: wave.bandRate ?? 0 } : {})
      }
    } catch (err) {
      console.warn('[audio] could not analyse', trackId, err)
      return null
    }
  }

  private snapshot(): Tick {
    const decks: DeckTick[] = []
    for (let deck = 0; this.engine && deck < DECKS; deck++) {
      const snap = this.engine.snapshot(deck)
      if (snap) decks.push(snap)
    }
    return { seq: this.seq, decks }
  }

  /** One tick soon, so a paused seek or a state change shows without waiting for the ticker. */
  private queueTick(): void {
    if (this.tickQueued) return
    this.tickQueued = true
    setImmediate(() => {
      this.tickQueued = false
      this.broadcast('deck:tick', this.snapshot())
    })
  }

  private startTicker(): void {
    this.idleTicks = 0
    if (this.ticker) return
    this.ticker = setInterval(() => this.tick(), TICK_MS)
  }

  private tick(): void {
    const engine = this.engine
    if (!engine) return
    this.broadcast('deck:tick', this.snapshot())
    if (engine.anySounding()) {
      this.idleTicks = 0
      return
    }
    engine.settle()
    if (++this.idleTicks < TAIL_TICKS) return
    if (this.ticker) clearInterval(this.ticker)
    this.ticker = null
  }

  private broadcast(channel: string, payload: unknown): void {
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.send(channel, payload)
    }
  }
}
