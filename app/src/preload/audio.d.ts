/** Commands the renderer sends to the deck engine. Times are fractional milliseconds. */
export type DeckCommand =
  | { op: 'load'; deck: number; trackId: string; request: number }
  | { op: 'unload'; deck: number }
  | { op: 'play'; deck: number }
  | { op: 'pause'; deck: number }
  | { op: 'seek'; deck: number; ms: number }
  /** Pause and move in one step, as releasing CUE does. */
  | { op: 'stopAt'; deck: number; ms: number }
  | { op: 'scrubBegin'; deck: number }
  /** `at` is the pointer event's `timeStamp`: the engine measures drag speed from these. */
  | { op: 'scrubTo'; deck: number; ms: number; at: number }
  | { op: 'scrubEnd'; deck: number }
  | { op: 'setLoop'; deck: number; inMs: number; outMs: number }
  | { op: 'setLooping'; deck: number; on: boolean }
  | { op: 'clearLoop'; deck: number }
  | { op: 'volume'; gain: number }

/** One deck as the engine last reported it. Frames are at `sampleRate`. */
export interface DeckTick {
  positionFrames: number
  totalFrames: number
  /** Bumped on every load, seek and scrub boundary: a deliberate move rather than drift. */
  generation: number
  sampleRate: number
  playing: boolean
  scrubbing: boolean
  loaded: boolean
  loadId: number
  loopInFrames: number
  loopOutFrames: number
  looping: boolean
}

/** Every deck at one instant. `seq` is the last command the engine had applied when it was taken. */
export interface Tick {
  seq: number
  decks: DeckTick[]
}

export interface DeckEvent {
  kind: 'loaded' | 'error'
  deck: number
  loadId: number
  totalFrames: number
  sampleRate: number
  message?: string | null
}

export type AudioStatus = { available: true } | { available: false; error: string }

/** High-quality waveform samples, analysed from the same decoder the deck plays through. */
export interface WaveAnalysis {
  /** Envelope entries per second. */
  rate: number
  data: Int16Array
  peak: number
  /** Low, mid, high peak per bin, 0-255, normalised so the loudest bin is 255. */
  bands?: Uint8Array
  bandRate?: number
}

export interface AudioApi {
  status(): Promise<AudioStatus>
  /** Sends a command and returns its sequence number, which later ticks echo back. */
  send(command: DeckCommand): number
  onTick(listener: (tick: Tick) => void): () => void
  onEvent(listener: (event: DeckEvent) => void): () => void
  analyse(trackId: string, bands: boolean): Promise<WaveAnalysis | null>
}
