import { app } from 'electron'
import { join, sep } from 'path'

export interface NativeDeckEvent {
  kind: 'loaded' | 'error'
  deck: number
  loadId: number
  totalFrames: number
  sampleRate: number
  message?: string | null
}

export interface NativeSnapshot {
  positionFrames: number
  totalFrames: number
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

export interface NativeWave {
  rate: number
  data: Int16Array
  peak: number
  bands?: Uint8Array | null
  bandRate?: number | null
}

export interface NativeEngine {
  readonly sampleRate: number
  load(deck: number, path: string, request: number): void
  unload(deck: number): void
  play(deck: number): void
  pause(deck: number): void
  seek(deck: number, ms: number): void
  stopAt(deck: number, ms: number): void
  scrubBegin(deck: number): void
  scrubTo(deck: number, ms: number, at: number): void
  scrubEnd(deck: number): void
  setLoop(deck: number, inMs: number, outMs: number): void
  setLooping(deck: number, on: boolean): void
  clearLoop(deck: number): void
  setVolume(gain: number): void
  snapshot(deck: number): NativeSnapshot | null
  anySounding(): boolean
  settle(): void
}

export interface NativeModule {
  AudioEngine: new (decks: number, onEvent: (event: NativeDeckEvent) => void) => NativeEngine
  analyse(path: string, bands: boolean): Promise<NativeWave | null>
}

/** Built by `npm run build:native` into resources/, which electron-builder unpacks from the asar. */
function addonPath(): string {
  return join(app.getAppPath(), 'resources', 'native', 'es_node.node').replace(
    `app.asar${sep}`,
    `app.asar.unpacked${sep}`
  )
}

export function loadNative(): NativeModule {
  const module = { exports: {} as NativeModule }
  process.dlopen(module, addonPath())
  return module.exports
}
