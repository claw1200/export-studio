import type { NativeLibrary, NativeModule } from './audio/native'

type Params = Record<string, unknown> | undefined

/** Methods the Rust library answers. Everything else, including every write, stays with the Python engine. */
const READS = new Set([
  'library.tracks',
  'library.playlists',
  'library.histories',
  'library.myTags',
  'library.playlistMembers',
  'playlist.tracks',
  'history.tracks'
])

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
 * Serves library reads from the Rust addon when `EXPORTSTUDIO_RUST_READS=1`. The addon reads a
 * snapshot, so it is reloaded before a read that follows any call that might have written.
 */
export class RustReads {
  private lib: NativeLibrary | null = null
  private dirty = false
  private opened = false

  constructor(private readonly addon: () => NativeModule | null) {}

  get enabled(): boolean {
    return process.env.EXPORTSTUDIO_RUST_READS === '1'
  }

  handles(method: string): boolean {
    return this.enabled && READS.has(method)
  }

  private library(): NativeLibrary {
    if (!this.lib) {
      const native = this.addon()
      if (!native?.RekordboxLibrary) throw new Error('The native library reader is not available')
      this.lib = new native.RekordboxLibrary()
    }
    return this.lib
  }

  /** Called after the Python engine answers, so the snapshot can follow what it did. */
  async after(method: string, params: Params): Promise<void> {
    if (!this.enabled) return
    if (method === 'library.open') {
      try {
        await this.library().open((params?.dbPath as string | undefined) ?? null)
        this.opened = true
        this.dirty = false
      } catch (e) {
        console.error('[rust-reads] open failed, using the Python engine', e)
        this.opened = false
      }
      return
    }
    if (!READS.has(method) && !PURE.has(method)) this.dirty = true
  }

  async call(method: string, params: Params): Promise<unknown> {
    if (!this.opened) throw new Error('The Rust library is not open')
    const lib = this.library()
    if (this.dirty) {
      await lib.reload()
      this.dirty = false
    }
    const id = String(params?.id ?? '')
    switch (method) {
      case 'library.tracks':
        return JSON.parse(await lib.tracks())
      case 'library.playlists':
        return JSON.parse(await lib.playlists())
      case 'library.histories':
        return JSON.parse(await lib.histories())
      case 'library.myTags':
        return JSON.parse(await lib.myTags())
      case 'library.playlistMembers':
        return JSON.parse(await lib.playlistMembers())
      case 'playlist.tracks':
        return JSON.parse(await lib.playlistTracks(id))
      case 'history.tracks':
        return JSON.parse(await lib.historyTracks(id))
      default:
        throw new Error(`Not a Rust read: ${method}`)
    }
  }
}
