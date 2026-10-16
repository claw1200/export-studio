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
  'history.tracks': (lib, p) => lib.historyTracks(text(p?.id))
}

/** Edits the Rust library makes. A failed edit is reported, never retried in Python. */
const WRITES: Record<string, (lib: NativeLibrary, p: Params) => Promise<string>> = {
  'track.removeFromCollection': (lib, p) => lib.removeTracks(list(p?.ids)),
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
  'tag.remove': (lib, p) => lib.tagRemove(text(p?.id), list(p?.trackIds))
}

/** Python calls that cannot change the master library; any other call may, so the Rust snapshot is refreshed after it. */
const PURE = new Set([
  'library.info',
  'track.detail',
  'track.previews',
  'track.inspect',
  'track.findMissing',
  'media.resolve',
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

  constructor(
    private readonly addon: () => NativeModule | null,
    private readonly refreshPython: () => Promise<unknown>
  ) {}

  get enabled(): boolean {
    return process.env.EXPORTSTUDIO_RUST_LIBRARY === '1'
  }

  kind(method: string): Kind | null {
    if (!this.enabled || !this.opened) return null
    if (method in READS) return 'read'
    if (method in WRITES) return 'write'
    return null
  }

  private library(): NativeLibrary {
    if (!this.lib) {
      const native = this.addon()
      if (!native?.RekordboxLibrary) throw new Error('The native library is not available')
      this.lib = new native.RekordboxLibrary()
    }
    return this.lib
  }

  /** Called after the Python engine answers, so the Rust side can follow what it did. */
  async after(method: string, params: Params, result: unknown): Promise<void> {
    if (!this.enabled) return
    if (method === 'library.open') {
      try {
        await this.library().open(text(params?.dbPath) || null)
        this.opened = true
        this.dirty = false
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
    if (method in WRITES) await this.refreshPython()
    return out
  }
}
