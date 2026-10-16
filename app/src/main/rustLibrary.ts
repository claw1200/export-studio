import type { NativeLibrary, NativeModule } from './audio/native'

type Params = Record<string, unknown> | undefined
type Kind = 'read' | 'write'

const text = (value: unknown): string => String(value ?? '')
const list = (value: unknown): string[] => (Array.isArray(value) ? value.map(String) : [])

/** Reads the Rust library answers; they may fall back to the Python engine on error. */
const READS: Record<string, (lib: NativeLibrary, p: Params) => Promise<string>> = {
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
    if (text(p?.id).startsWith('usb:')) {
      return text(p?.kind) === 'audio' ? lib.usbMedia(text(p?.id)) : Promise.resolve('null')
    }
    return lib.mediaResolve(text(p?.kind), text(p?.id), text(p?.size))
  },
  'track.previews': (lib, p) => lib.trackPreviews(list(p?.ids)),
  'usb.list': (lib) => lib.usbList(),
  'usb.playlists': (lib, p) => lib.usbPlaylists(text(p?.volumeId)),
  'usb.libraries': (lib, p) => lib.usbLibraries(text(p?.volumeId)),
  'usb.selection': (lib, p) => lib.usbSelection(text(p?.volumeId)),
  'usb.tracks': (lib, p) =>
    lib.usbTracks(text(p?.volumeId), p?.playlistId == null ? null : text(p.playlistId)),
  'usb.detail': (lib, p) => lib.usbDetail(text(p?.id)),
  'usb.importPreview': (lib, p) =>
    lib.usbImportPreview(
      text(p?.volumeId),
      p?.folder == null ? null : text(p.folder),
      p?.policy == null ? null : text(p.policy)
    )
}

/** Edits the Rust library makes. A failed edit is reported, never retried in Python. */
const WRITES: Record<string, (lib: NativeLibrary, p: Params) => Promise<string>> = {
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
  'track.update': (lib, p) => lib.trackUpdate(list(p?.ids), JSON.stringify(p?.fields ?? {})),
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
  'usb.eject': (lib, p) => lib.usbEject(text(p?.volumeId)),
  'usb.format': (lib, p) => lib.usbFormat(text(p?.volumeId)),
  'usb.sync': (lib, p) => lib.usbSync(text(p?.volumeId), list(p?.playlistIds), Boolean(p?.overwrite)),
  'usb.import': (lib, p) =>
    lib.usbImport(
      text(p?.volumeId),
      p?.folder == null ? null : text(p.folder),
      p?.policy == null ? null : text(p.policy)
    )
}

/** Edits that leave the master library alone, so the Python engine has nothing to reload after them. */
const KEEPS_DATABASE = new Set([
  'backup.create',
  'backup.delete',
  'backup.setSettings',
  'usb.eject',
  'usb.format',
  'usb.sync'
])

const DAILY_CHECK_MS = 10 * 60 * 1000

type Listener = (event: string, data: unknown) => void

/** Python calls that cannot change the master library; any other call may, so the Rust snapshot is refreshed after it. */
const PURE = new Set([
  'library.info',
  'track.previews',
  'track.inspect',
  'track.findMissing',
  'files.read',
  'usb.list',
  'usb.eject',
  'usb.playlists',
  'usb.libraries',
  'usb.selection',
  'usb.tracks',
  'usb.detail',
  'usb.sync',
  'usb.format',
  'usb.importPreview',
  'backup.list',
  'backup.settings',
  'backup.setSettings'
])

/**
 * Serves the library calls it can from the Rust addon when `EXPORTSTUDIO_RUST_LIBRARY=1`; the rest
 * stay with the Python engine. The two engines share one database, so each is told when the other
 * may have changed it: the Rust snapshot reloads before a read that follows a Python call that might
 * have written, and the Python engine reloads after every Rust edit.
 */
export class RustLibrary {
  private lib: NativeLibrary | null = null
  private dirty = false
  private opened = false
  private daily: ReturnType<typeof setInterval> | null = null
  private readonly listeners = new Set<Listener>()

  constructor(
    private readonly addon: () => NativeModule | null,
    private readonly refreshPython: () => Promise<unknown>
  ) {}

  get enabled(): boolean {
    return process.env.EXPORTSTUDIO_RUST_LIBRARY === '1'
  }

  kind(method: string, params?: Params): Kind | null {
    if (!this.enabled || !this.opened) return null
    // Writing tags into audio files is still done by the Python engine.
    if (method === 'track.update' && params?.writeTags) return null
    if (method in READS) return 'read'
    if (method in WRITES) return 'write'
    return null
  }

  private library(): NativeLibrary {
    if (!this.lib) {
      const native = this.addon()
      if (!native?.RekordboxLibrary) throw new Error('The native library is not available')
      this.lib = new native.RekordboxLibrary()
      this.lib.onEvent((event, data) => {
        let parsed: unknown = {}
        try {
          parsed = JSON.parse(data)
        } catch {
          // An event without a readable payload is still worth passing on.
        }
        for (const listener of this.listeners) listener(event, parsed)
      })
    }
    return this.lib
  }

  /** Events from the Rust side: backup progress and changes, and library reloads. */
  onEvent(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** Rekordbox started and the Python engine left write mode, so editing is locked here too. */
  async locked(): Promise<void> {
    if (this.opened) await this.library().setWritable(false)
  }

  private startDaily(): void {
    if (this.daily) return
    const check = (): void => {
      this.library()
        .dailyBackup()
        .catch((e) => console.error('[rust-library] daily backup failed', e))
    }
    this.daily = setInterval(check, DAILY_CHECK_MS)
    this.daily.unref()
    check()
  }

  /** Called after the Python engine answers, so the Rust side can follow what it did. */
  async after(method: string, params: Params, result: unknown): Promise<void> {
    if (!this.enabled) return
    if (method === 'library.open') {
      try {
        await this.library().open(text(params?.dbPath) || null)
        this.opened = true
        this.dirty = false
        this.startDaily()
      } catch (e) {
        console.error('[rust-library] open failed, using the Python engine', e)
        this.opened = false
      }
      return
    }
    if (method === 'library.setWritable' && this.opened) {
      await this.library().setWritable(Boolean((result as { writable?: boolean } | null)?.writable))
      return
    }
    if (method in READS || method in WRITES || PURE.has(method)) return
    this.dirty = true
  }

  async call(method: string, params: Params): Promise<unknown> {
    const lib = this.library()
    if (this.dirty) {
      await lib.reload()
      this.dirty = false
    }
    const run = READS[method] ?? WRITES[method]
    const out = JSON.parse(await run(lib, params))
    if (method in WRITES && !KEEPS_DATABASE.has(method)) await this.refreshPython()
    return out
  }
}
