import { app } from 'electron'
import { is } from '@electron-toolkit/utils'
import { join, resolve } from 'path'
import type { NativeLibrary, NativeModule } from './audio/native'
import type { EngineCommandParams, EngineMethod } from '../preload/engine'

type CommandHandlers = {
  [Method in EngineMethod]?: (lib: NativeLibrary, params: EngineCommandParams[Method]) => Promise<unknown>
}

const text = (value: unknown): string => String(value ?? '')
const list = (value: unknown): string[] => (Array.isArray(value) ? value.map(String) : [])

/** Calls that only read the library. */
const READS: CommandHandlers = {
  'library.tracks': (lib) => lib.tracks(),
  'library.playlists': (lib) => lib.playlists(),
  'library.histories': (lib) => lib.histories(),
  'library.myTags': (lib) => lib.myTags(),
  'library.playlistMembers': (lib) => lib.playlistMembers(),
  'playlist.tracks': (lib, p) => lib.playlistTracks(text(p?.id)),
  'history.tracks': (lib, p) => lib.historyTracks(text(p?.id)),
  'backup.list': (lib) => lib.backupList(),
  'backup.settings': (lib) => lib.backupSettings(),
  'track.detail': (lib, p) =>
    text(p?.id).startsWith('usb:') ? lib.usbDetail(text(p?.id)) : lib.trackDetail(text(p?.id)),
  'media.resolve': (lib, p) => {
    if (text(p?.id).startsWith('usb:') && text(p?.kind) === 'audio') {
      return lib.usbMedia(text(p?.id))
    }
    return lib.mediaResolve(text(p?.kind), text(p?.id), text(p?.size))
  },
  'track.previews': (lib, p) => lib.trackPreviews(list(p?.ids)),
  'files.read': (lib, p) => lib.filesRead(list(p?.paths)),
  'track.findMissing': (lib, p) => lib.trackFindMissing(list(p?.ids), text(p?.directory)),
  'track.inspect': (lib, p) => lib.trackInspect(JSON.stringify(p?.tracks ?? [])),
  'usb.list': (lib) => lib.usbList(),
  'usb.playlists': (lib, p) => lib.usbPlaylists(text(p?.volumeId)),
  'usb.libraries': (lib, p) => lib.usbLibraries(text(p?.volumeId)),
  'usb.selection': (lib, p) => lib.usbSelection(text(p?.volumeId)),
  'usb.tracks': (lib, p) => lib.usbTracks(text(p?.volumeId), p?.playlistId == null ? null : text(p.playlistId)),
  'usb.detail': (lib, p) => lib.usbDetail(text(p?.id)),
  'usb.importPreview': (lib, p) =>
    lib.usbImportPreview(
      text(p?.volumeId),
      p?.folder == null ? null : text(p.folder),
      p?.policy == null ? null : text(p.policy)
    )
}

/** Calls that change the library, or other things on disk. */
const WRITES: CommandHandlers = {
  'backup.create': (lib) => lib.backupCreate(),
  'backup.delete': (lib, p) => lib.backupDelete(text(p?.id)),
  'backup.restore': (lib, p) => lib.backupRestore(text(p?.id)),
  'backup.setSettings': (lib, p) =>
    lib.backupSetSettings(
      p?.auto == null ? null : Boolean(p.auto),
      p?.keep == null ? null : Number(p.keep),
      p?.interval == null ? null : Number(p.interval)
    ),
  'track.removeFromCollection': (lib, p) => lib.removeTracks(list(p?.ids)),
  'track.update': (lib, p) => lib.trackUpdate(list(p?.ids), JSON.stringify(p?.fields ?? {}), Boolean(p?.writeTags)),
  'track.import': (lib, p) =>
    lib.trackImport(list(p?.paths), p?.playlistId == null ? null : text(p.playlistId), p?.artwork !== false),
  'cue.setHot': (lib, p) => lib.cueSetHot(text(p?.id), Number(p?.kind), Number(p?.ms)),
  'cue.addMemory': (lib, p) => lib.cueAddMemory(text(p?.id), Number(p?.ms)),
  'cue.delete': (lib, p) => lib.cueDelete(text(p?.id), text(p?.cueId)),
  'cue.setColor': (lib, p) =>
    lib.cueSetColor(text(p?.id), text(p?.cueId), p?.colorIndex == null ? null : Number(p.colorIndex)),
  'playlist.add': (lib, p) => lib.playlistAdd(text(p?.id), list(p?.trackIds)),
  'playlist.remove': (lib, p) => lib.playlistRemove(text(p?.id), list(p?.trackIds)),
  'playlist.move': (lib, p) => lib.playlistMove(text(p?.id), list(p?.trackIds), Number(p?.index ?? 0)),
  'playlist.create': (lib, p) =>
    lib.playlistCreate(
      text(p?.name),
      p?.parentId == null ? null : text(p.parentId),
      Boolean(p?.folder),
      list(p?.trackIds)
    ),
  'playlist.rename': (lib, p) => lib.playlistRename(text(p?.id), text(p?.name)),
  'playlist.delete': (lib, p) => lib.playlistDelete(text(p?.id)),
  'tag.add': (lib, p) => lib.tagAdd(text(p?.id), list(p?.trackIds)),
  'tag.remove': (lib, p) => lib.tagRemove(text(p?.id), list(p?.trackIds)),
  'beatgrid.set': (lib, p) =>
    lib.beatgridSet(
      text(p?.id),
      JSON.stringify({ times: p?.times, beats: p?.beats, bpms: p?.bpms, cues: p?.cues ?? [] })
    ),
  'track.applyAnalysis': (lib, p) =>
    lib.trackApplyAnalysis(text(p?.id), JSON.stringify({ result: p?.result, options: p?.options })),
  'track.convert': (lib, p) => lib.trackConvert(list(p?.ids), JSON.stringify(p?.preset ?? {}), text(p?.mode)),
  'track.relocate': (lib, p) => lib.trackRelocate(JSON.stringify(p?.moves ?? {})),
  'usb.eject': (lib, p) => lib.usbEject(text(p?.volumeId)),
  'usb.format': (lib, p) => lib.usbFormat(text(p?.volumeId)),
  'usb.sync': (lib, p) => lib.usbSync(text(p?.volumeId), list(p?.playlistIds)),
  'usb.import': (lib, p) =>
    lib.usbImport(
      text(p?.volumeId),
      p?.folder == null ? null : text(p.folder),
      p?.policy == null ? null : text(p.policy)
    )
}

const LIFECYCLE: CommandHandlers = {
  'library.info': (lib) => lib.info(),
  'library.setWritable': (lib, p) => lib.setWritable(Boolean(p?.writable))
}

function isEngineMethod(method: string): method is EngineMethod {
  return (
    method === 'library.open' ||
    method === 'library.reload' ||
    Object.prototype.hasOwnProperty.call(READS, method) ||
    Object.prototype.hasOwnProperty.call(WRITES, method) ||
    Object.prototype.hasOwnProperty.call(LIFECYCLE, method)
  )
}

const DAILY_CHECK_MS = 10 * 60 * 1000
const WATCH_MS = 2000

type Listener = (event: string, data: unknown) => void

export class LibraryError extends Error {
  constructor(
    message: string,
    public type = 'Error'
  ) {
    super(message)
  }
}

/** The USB export program next to the app, or in the vendored build tree in development. */
export function usbExport(): string {
  const name = process.platform === 'win32' ? 'es-usb.exe' : 'es-usb'
  if (is.dev) return resolve(app.getAppPath(), '..', 'vendor', 'usb-sync', 'target', 'release', name)
  return join(process.resourcesPath, 'engine', name)
}

export function ffmpegBinary(): string {
  const name = process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg'
  if (is.dev) return resolve(app.getAppPath(), 'resources', 'native', name)
  return join(process.resourcesPath, 'engine', name)
}

interface Info {
  rekordboxRunning: boolean
  sourceChanged: boolean
  writable: boolean
}

interface Volume {
  id: string
  hasLibrary: boolean
  rootName: string | null
  ejectable: boolean
}

/** The rekordbox library, answered by the native addon, and the events that follow it: backups, USB volumes, rekordbox starting. */
export class Library {
  private lib: NativeLibrary | null = null
  private opened = false
  private daily: ReturnType<typeof setInterval> | null = null
  private watcher: ReturnType<typeof setInterval> | null = null
  private volumes = ''
  private running: boolean | null = null
  private notified = false
  private lockRefreshPending = false
  private readonly listeners = new Set<Listener>()

  constructor(private readonly addon: () => NativeModule | null) {}

  private library(): NativeLibrary {
    if (!this.lib) {
      const native = this.addon()
      if (!native?.RekordboxLibrary) throw new LibraryError('The native library is not available')
      this.lib = new native.RekordboxLibrary()
      this.lib.onEvent((event, data) => {
        let parsed: unknown = {}
        try {
          parsed = JSON.parse(data)
        } catch {
          // An event without a readable payload is still worth passing on.
        }
        this.emit(event, parsed)
      })
    }
    return this.lib
  }

  private emit(event: string, data: unknown): void {
    for (const listener of this.listeners) listener(event, data)
  }

  /** Events from the library: backup progress and changes, library reloads, USB volumes and progress. */
  onEvent(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** Starts watching USB volumes and rekordbox; safe to call twice. */
  start(): void {
    if (this.watcher) return
    this.watcher = setInterval(() => void this.watch(), WATCH_MS)
    this.watcher.unref()
  }

  stop(): void {
    if (this.watcher) clearInterval(this.watcher)
    if (this.daily) clearInterval(this.daily)
    this.watcher = this.daily = null
  }

  private async watch(): Promise<void> {
    try {
      const lib = this.library()
      const volumes = (await lib.usbList()) as Volume[]
      const key = JSON.stringify(volumes.map((v) => [v.id, v.hasLibrary, v.rootName, v.ejectable]))
      if (this.volumes && key !== this.volumes) this.emit('usb.volumes', { volumes })
      this.volumes = key
      if (!this.opened) return
      let info = (await lib.info()) as Info
      if (info.writable && info.rekordboxRunning) {
        this.lockRefreshPending = true
        try {
          info = (await lib.setWritable(false)) as Info
          this.lockRefreshPending = false
          this.emit('library.locked', info)
        } catch (e) {
          info = (await lib.info()) as Info
          this.emit('library.locked', info)
          console.error('[library] lock snapshot refresh failed; will retry', e)
        }
        this.notified = info.sourceChanged
      } else if (this.lockRefreshPending && !info.writable) {
        try {
          info = (await lib.setWritable(false)) as Info
          this.lockRefreshPending = false
          this.emit('library.locked', info)
          this.notified = info.sourceChanged
        } catch {
          // Keep the pending refresh for the next watcher poll.
        }
      } else if (this.running !== null && info.rekordboxRunning !== this.running) {
        this.emit('library.rekordbox', { running: info.rekordboxRunning })
      }
      this.running = info.rekordboxRunning
      if (info.sourceChanged && !this.notified) this.emit('library.changed', {})
      this.notified = info.sourceChanged
    } catch (e) {
      console.error('[library] watch failed', e)
    }
  }

  private startDaily(): void {
    if (this.daily) return
    const check = (): void => {
      this.library()
        .dailyBackup()
        .catch((e) => console.error('[library] daily backup failed', e))
    }
    this.daily = setInterval(check, DAILY_CHECK_MS)
    this.daily.unref()
    check()
  }

  async call<T = unknown>(method: EngineMethod, params?: EngineCommandParams[EngineMethod]): Promise<T> {
    if (!isEngineMethod(method)) throw new LibraryError(`Unknown method: ${method}`)
    try {
      if (method === 'library.open') {
        const openParams = params as EngineCommandParams['library.open']
        const info = await this.library().open(text(openParams?.dbPath) || null)
        this.opened = true
        this.running = null
        this.notified = false
        this.lockRefreshPending = false
        this.startDaily()
        return info as T
      }
      if (method === 'library.reload') {
        const info = await this.library().reload()
        this.lockRefreshPending = false
        this.emit('library.reloaded', {})
        return info as T
      }
      const handler = READS[method] ?? WRITES[method] ?? LIFECYCLE[method]
      if (!handler) throw new LibraryError(`Unknown method: ${method}`)
      const run = handler as (lib: NativeLibrary, params: unknown) => Promise<unknown>
      return (await run(this.library(), params)) as T
    } catch (e) {
      if (e instanceof LibraryError) throw e
      throw new LibraryError((e as Error).message)
    }
  }
}
