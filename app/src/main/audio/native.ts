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
  tempo: number
  masterTempo: boolean
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
  setLoop(deck: number, inMs: number, outMs: number, enter: boolean): void
  setLooping(deck: number, on: boolean): void
  clearLoop(deck: number): void
  setTempo(deck: number, tempo: number): void
  setMasterTempo(deck: number, on: boolean): void
  setVolume(gain: number): void
  snapshot(deck: number): NativeSnapshot | null
  anySounding(): boolean
  settle(): void
  close(): void
}

export interface NativeTrackAnalysisOptions {
  grid: boolean
  key: boolean
  dynamic: boolean
  minBpm: number
  maxBpm: number
  highPrecision: boolean
  roundBpm: boolean
}

export interface NativeTrackAnalysis {
  duration: number
  /** 0 when no grid was found or asked for. */
  bpm: number
  /** Per beat: its number in the bar (1 to 4), tempo × 100, and time in ms. */
  beatNumbers: Uint8Array
  beatTempos: Uint16Array
  beatTimes: Uint32Array
  /** Tonic 0 is C. */
  key?: { tonic: number; minor: boolean } | null
  /** Eight bytes per 1/150 s: peak and RMS of the whole signal, then the low, mid and high bands. */
  waveform: Uint8Array
}

export interface NativeLibrary {
  open(path?: string | null): Promise<unknown>
  reload(): Promise<unknown>
  info(): Promise<unknown>
  tracks(): Promise<unknown>
  playlists(): Promise<unknown>
  histories(): Promise<unknown>
  myTags(): Promise<unknown>
  playlistMembers(): Promise<unknown>
  playlistTracks(id: string): Promise<unknown>
  historyTracks(id: string): Promise<unknown>
  trackDetail(id: string): Promise<unknown>
  mediaResolve(kind: string, id: string, size: string): Promise<unknown>
  cueSetHot(id: string, kind: number, ms: number): Promise<unknown>
  cueAddMemory(id: string, ms: number): Promise<unknown>
  cueDelete(id: string, cueId: string): Promise<unknown>
  cueSetColor(id: string, cueId: string, colorIndex: number | null): Promise<unknown>
  onEvent(callback: (event: string, data: string) => void): void
  backupList(): Promise<unknown>
  backupCreate(): Promise<unknown>
  backupDelete(id: string): Promise<unknown>
  backupRestore(id: string): Promise<unknown>
  backupSettings(): Promise<unknown>
  backupSetSettings(auto: boolean | null, keep: number | null, interval: number | null): Promise<unknown>
  dailyBackup(): Promise<unknown>
  trackUpdate(ids: string[], fields: string, writeTags?: boolean): Promise<unknown>
  trackImport(paths: string[], playlistId: string | null, artwork: boolean): Promise<unknown>
  setWritable(on: boolean): Promise<unknown>
  trackPreviews(ids: string[]): Promise<unknown>
  filesRead(paths: string[]): Promise<unknown>
  beatgridSet(id: string, body: string): Promise<unknown>
  trackConvert(ids: string[], preset: string, mode: string): Promise<unknown>
  trackApplyAnalysis(id: string, body: string): Promise<unknown>
  trackRelocate(moves: string): Promise<unknown>
  trackFindMissing(ids: string[], directory: string): Promise<unknown>
  trackInspect(tracks: string): Promise<unknown>
  usbList(): Promise<unknown>
  usbEject(volume: string): Promise<unknown>
  usbFormat(volume: string): Promise<unknown>
  usbPlaylists(volume: string): Promise<unknown>
  usbLibraries(volume: string): Promise<unknown>
  usbSelection(volume: string): Promise<unknown>
  usbTracks(volume: string, playlistId: string | null): Promise<unknown>
  usbDetail(key: string): Promise<unknown>
  usbMedia(key: string): Promise<unknown>
  usbImportPreview(volume: string, folder: string | null, policy: string | null): Promise<unknown>
  usbImport(volume: string, folder: string | null, policy: string | null): Promise<unknown>
  usbSyncReview(volume: string, playlistIds: string[]): Promise<unknown>
  usbSync(volume: string, playlistIds: string[], review?: unknown): Promise<unknown>
  usbRecoveryStatus(volume: string): Promise<unknown>
  usbRecover(volume: string): Promise<unknown>
  removeTracks(ids: string[]): Promise<unknown>
  playlistAdd(id: string, trackIds: string[]): Promise<unknown>
  playlistRemove(id: string, trackIds: string[]): Promise<unknown>
  playlistMove(id: string, trackIds: string[], index: number): Promise<unknown>
  playlistCreate(name: string, parentId: string | null, folder: boolean, trackIds: string[]): Promise<unknown>
  playlistRename(id: string, name: string): Promise<unknown>
  playlistDelete(id: string): Promise<unknown>
  tagAdd(tagId: string, trackIds: string[]): Promise<unknown>
  tagRemove(tagId: string, trackIds: string[]): Promise<unknown>
}

export interface NativeModule {
  AudioEngine: new (decks: number, onEvent: (event: NativeDeckEvent) => void) => NativeEngine
  RekordboxLibrary: new () => NativeLibrary
  analyse(path: string, bands: boolean, quality: number): Promise<NativeWave | null>
  analyseTrack(path: string, options: NativeTrackAnalysisOptions): Promise<NativeTrackAnalysis>
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
