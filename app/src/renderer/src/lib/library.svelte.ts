import { api } from './api'
import { browserPreviews } from './browserPreview.svelte'
import { clearDetailCache } from './deck/session.svelte'
import { cueStats, newestFirst, type SortKey, sortTracks, type TrackSort } from './library/sort'
import { buildTree } from './library/tree'
import { loadViews, looseTrack, matchesTrack, pathKey, saveViews, type FileEntry } from './library/views'
import type {
  Cue,
  ImportResult,
  LibraryInfo,
  Source,
  SyncProgress,
  Track,
  TrackEdit,
  TreeItem,
  View,
  Volume
} from './types'

export const TRACK_DRAG_TYPE = 'application/x-exportstudio-tracks'

/** How cover art is drawn in the browser. Fill uses the column width and crops the bottom. */
export type BrowserArtwork = 'fill' | 'whole'

function storedArtwork(): BrowserArtwork {
  return localStorage.getItem('browserArtwork') === 'whole' ? 'whole' : 'fill'
}

function dropCaches(): void {
  clearDetailCache()
  browserPreviews.forget()
}

const READ_ONLY = 'Unlock editing to change the library'
const NOTICE_MS = 6000

export class Library {
  tracks = $state.raw<Track[]>([])
  playlists = $state.raw<TreeItem[]>([])
  histories = $state.raw<TreeItem[]>([])
  myTags = $state.raw<TreeItem[]>([])
  info = $state.raw<LibraryInfo | null>(null)
  devices = $state.raw<Volume[]>([])
  devicePlaylists = $state.raw<Record<string, TreeItem[]>>({})
  deviceTracks = $state.raw<Track[]>([])
  views = $state.raw<View[]>(loadViews())
  playlistMembers = $state.raw<ReadonlySet<string>>(new Set())
  /** Files under the open file view, in directory order. */
  fileEntries = $state.raw<FileEntry[]>([])
  syncing = $state(false)
  syncProgress = $state<SyncProgress | null>(null)
  syncError = $state<string | null>(null)

  status = $state<'idle' | 'loading' | 'ready' | 'error'>('idle')
  error = $state<string | null>(null)
  changed = $state(false)
  switching = $state(false)
  lockError = $state<string | null>(null)

  writable = $derived(this.info?.writable ?? false)

  source = $state.raw<Source>({ type: 'collection' })
  sourceIds = $state.raw<string[] | null>(null)
  sourceLoading = $state(false)
  query = $state('')
  sort = $state.raw<TrackSort | null>(null)
  selectedId = $state<string | null>(null)
  selection = $state.raw<ReadonlySet<string>>(new Set())
  private anchorId: string | null = null

  editError = $state<string | null>(null)
  writeFileTags = $state(localStorage.getItem('writeFileTags') === '1')
  importArtwork = $state(localStorage.getItem('importArtwork') !== '0')
  hideStreaming = $state(localStorage.getItem('hideStreaming') !== '0')
  browserArtwork = $state<BrowserArtwork>(storedArtwork())
  importing = $state(false)
  renamingId = $state<string | null>(null)
  dragging = $state.raw<string[] | null>(null)
  private noticeTimer: ReturnType<typeof setTimeout> | undefined
  private trackListeners = new Set<(tracks: Track[]) => void>()
  private importListeners = new Set<(tracks: Track[]) => void>()
  private removeListeners = new Set<(ids: string[]) => void>()

  viewById = $derived(new Map(this.views.map((view) => [view.id, view])))
  private looseTracks = $derived(this.fileEntries.flatMap((entry) => (entry.loose ? [entry.loose] : [])))
  byId = $derived(new Map([...this.tracks, ...this.deviceTracks, ...this.looseTracks].map((t) => [t.id, t])))
  playlistById = $derived(new Map(this.playlists.map((p) => [p.id, p])))
  playlistTree = $derived(buildTree(this.playlists))
  historyTree = $derived(buildTree(this.histories))
  tagTree = $derived(buildTree(this.myTags))
  tagNames = $derived(new Map(this.myTags.map((t) => [t.id, t.name])))
  missingTracks = $derived(this.tracks.filter((t) => t.missing))
  tagCounts = $derived.by(() => {
    const counts = new Map<string, number>()
    for (const t of this.tracks) for (const id of t.myTags) counts.set(id, (counts.get(id) ?? 0) + 1)
    return counts
  })

  private haystack = $derived(
    new Map(
      [...this.tracks, ...this.deviceTracks, ...this.looseTracks].map((t) => [
        t.id,
        [t.title, t.artist, t.album, t.genre, t.label, t.remixer, t.comment, t.key, t.originalArtist]
          .join('\u0001')
          .toLowerCase()
      ])
    )
  )

  /** 1-based position of each track in the current playlist or history, as stored in rekordbox. */
  positions = $derived.by(() => {
    const out = new Map<string, number>()
    if (this.source.type === 'device' && this.source.playlistId) {
      this.deviceTracks.forEach((track, i) => {
        if (!out.has(track.id)) out.set(track.id, i + 1)
      })
      return out
    }
    if (this.source.type !== 'playlist' && this.source.type !== 'history') return out
    this.sourceIds?.forEach((id, i) => {
      if (!out.has(id)) out.set(id, i + 1)
    })
    return out
  })

  private allSourceTracks = $derived.by(() => {
    const src = this.source
    if (src.type === 'collection') return this.tracks
    if (src.type === 'device') return this.deviceTracks
    if (src.type === 'tag') return this.tracks.filter((t) => t.myTags.includes(src.id))
    if (src.type === 'view') return this.viewTracks(src.id)
    if (!this.sourceIds) return []
    const out: Track[] = []
    for (const id of this.sourceIds) {
      const t = this.byId.get(id)
      if (t) out.push(t)
    }
    return out
  })

  sourceTracks = $derived(this.hideStreaming ? this.allSourceTracks.filter((t) => !t.streaming) : this.allSourceTracks)

  visible = $derived.by(() => {
    let list = this.sourceTracks
    const terms = this.query.toLowerCase().split(/\s+/).filter(Boolean)
    if (terms.length) {
      list = list.filter((t) => {
        const h = this.haystack.get(t.id) ?? ''
        return terms.every((term) => h.includes(term))
      })
    }
    if (this.sort) return sortTracks(list, this.sort, this.positions)
    if (this.source.type === 'collection' || this.source.type === 'tag') return newestFirst(list)
    if (this.source.type === 'view' && this.viewById.get(this.source.id)?.type === 'tracks') return newestFirst(list)
    return list
  })

  selected = $derived(this.selectedId ? (this.byId.get(this.selectedId) ?? null) : null)
  selectedTracks = $derived.by(() => {
    const picked = this.visible.filter((t) => this.selection.has(t.id))
    return picked.length || !this.selected ? picked : [this.selected]
  })

  sourcePlaylist = $derived(this.source.type === 'playlist' ? this.playlistById.get(this.source.id) : undefined)
  canReorder = $derived(
    this.writable &&
      this.sourcePlaylist?.kind === 'playlist' &&
      (!this.sort || (this.sort.key === 'position' && this.sort.dir === 1)) &&
      !this.query.trim()
  )

  sourceTitle = $derived.by(() => {
    const src = this.source
    if (src.type === 'collection') return 'Collection'
    if (src.type === 'device') {
      const volume = this.devices.find((item) => item.id === src.volumeId)
      if (!src.playlistId) return volume?.name ?? 'Device'
      return (
        this.devicePlaylists[src.volumeId]?.find((item) => item.id === src.playlistId)?.name ?? volume?.name ?? 'Device'
      )
    }
    if (src.type === 'view') return this.viewById.get(src.id)?.name ?? 'View'
    const list = src.type === 'playlist' ? this.playlists : src.type === 'history' ? this.histories : this.myTags
    return list.find((i) => i.id === src.id)?.name ?? ''
  })

  private sourceToken = 0

  constructor() {
    api.onEvent((event, data) => {
      if (event === 'library.changed') this.changed = true
      if (event === 'usb.volumes') void this.refreshDevices()
      if (event === 'usb.progress') this.syncProgress = data as SyncProgress
      if (event === 'library.rekordbox') {
        const running = Boolean((data as { running?: boolean } | undefined)?.running)
        if (this.info && this.info.rekordboxRunning !== running) {
          this.info = { ...this.info, rekordboxRunning: running }
        }
        if (!running) this.lockError = null
      }
      if (event === 'library.locked') {
        dropCaches()
        this.info = data as LibraryInfo
        this.changed = false
        this.lockError = null
        void this.refreshContents()
      }
    })
  }

  async load(reload = false): Promise<void> {
    this.status = 'loading'
    this.error = null
    if (reload) dropCaches()
    try {
      this.info = reload ? await api.reload() : await api.open()
      await this.refreshContents()
      await this.refreshDevices()
      this.status = 'ready'
    } catch (e) {
      this.error = (e as Error).message
      this.status = 'error'
    }
  }

  async setWritable(on: boolean): Promise<boolean> {
    if (this.switching || on === this.writable) return false
    if (on && this.info?.rekordboxRunning) {
      this.lockError = 'Close rekordbox before editing the library'
      return false
    }
    this.switching = true
    this.lockError = null
    try {
      this.info = await api.setWritable(on)
      dropCaches()
      await this.refreshContents()
      return this.writable === on
    } catch (e) {
      this.lockError = (e as Error).message
      try {
        const info = await api.info()
        const reopened = info.writable !== this.writable || info.snapshotTime !== this.info?.snapshotTime
        this.info = info
        if (reopened) {
          dropCaches()
          await this.refreshContents()
        }
      } catch {
        return false
      }
      return false
    } finally {
      this.switching = false
    }
  }

  setTrackBpm(id: string, bpm: number): void {
    if (this.byId.get(id)?.bpm === bpm) return
    this.tracks = this.tracks.map((t) => (t.id === id ? { ...t, bpm } : t))
  }

  selectOnly(id: string): void {
    this.selectedId = id
    this.selection = new Set([id])
    this.anchorId = id
  }

  toggleSelected(id: string): void {
    const next = new Set(this.selection)
    if (next.has(id) && next.size > 1) {
      next.delete(id)
      if (this.selectedId === id) this.selectedId = [...next].at(-1) ?? null
    } else {
      next.add(id)
      this.selectedId = id
    }
    this.selection = next
    this.anchorId = id
  }

  selectRange(id: string): void {
    const list = this.visible
    const from = list.findIndex((t) => t.id === (this.anchorId ?? this.selectedId))
    const to = list.findIndex((t) => t.id === id)
    if (from < 0 || to < 0) return this.selectOnly(id)
    const [lo, hi] = from < to ? [from, to] : [to, from]
    this.selection = new Set(list.slice(lo, hi + 1).map((t) => t.id))
    this.selectedId = id
  }

  selectAll(): void {
    const list = this.visible
    if (!list.length) return
    this.selection = new Set(list.map((t) => t.id))
    if (!this.selectedId || !this.selection.has(this.selectedId)) this.selectedId = list[0].id
  }

  setWriteFileTags(on: boolean): void {
    this.writeFileTags = on
    localStorage.setItem('writeFileTags', on ? '1' : '0')
  }

  setImportArtwork(on: boolean): void {
    this.importArtwork = on
    localStorage.setItem('importArtwork', on ? '1' : '0')
  }

  setHideStreaming(on: boolean): void {
    this.hideStreaming = on
    localStorage.setItem('hideStreaming', on ? '1' : '0')
  }

  setBrowserArtwork(style: BrowserArtwork): void {
    this.browserArtwork = style
    localStorage.setItem('browserArtwork', style)
  }

  onTracksUpdated(listener: (tracks: Track[]) => void): () => void {
    this.trackListeners.add(listener)
    return () => this.trackListeners.delete(listener)
  }

  /** Called with the tracks an import added to the collection. */
  onImported(listener: (tracks: Track[]) => void): () => void {
    this.importListeners.add(listener)
    return () => this.importListeners.delete(listener)
  }

  onRemoved(listener: (ids: string[]) => void): () => void {
    this.removeListeners.add(listener)
    return () => this.removeListeners.delete(listener)
  }

  /** Apply rows from outside an edit, such as finished analysis. */
  updateRows(rows: Track[]): void {
    this.applyTracks(rows)
  }

  /** Refresh the CUE and waveform sort keys after cues are saved. */
  noteCueCounts(id: string, cues: Cue[]): void {
    const track = this.byId.get(id)
    if (!track) return
    const stats = cueStats(cues, track.phrase)
    if (stats.cueStars === track.cueStars && stats.cueCount === track.cueCount) return
    const fresh = { ...track, ...stats }
    if (id.startsWith('usb:')) this.deviceTracks = this.deviceTracks.map((row) => (row.id === id ? fresh : row))
    else this.applyTracks([fresh])
  }

  notify(message: string): void {
    clearTimeout(this.noticeTimer)
    this.editError = message
    this.noticeTimer = setTimeout(() => {
      if (this.editError === message) this.editError = null
    }, NOTICE_MS)
  }

  private async edit<T>(run: () => Promise<T>): Promise<T | undefined> {
    if (!this.writable) {
      this.notify(READ_ONLY)
      return undefined
    }
    clearTimeout(this.noticeTimer)
    this.editError = null
    try {
      return await run()
    } catch (e) {
      this.notify((e as Error).message)
      return undefined
    }
  }

  private applyTracks(rows: Track[]): void {
    if (!rows.length) return
    const fresh = new Map(rows.map((r) => [r.id, r]))
    this.tracks = this.tracks.map((t) => fresh.get(t.id) ?? t)
    for (const listener of this.trackListeners) listener(rows)
    if (!this.sourceLoading) void this.fillFileGaps()
  }

  setMissing(id: string, missing: boolean): void {
    const track = this.byId.get(id)
    if (!track || track.missing === missing) return
    const fresh = { ...track, missing }
    if (id.startsWith('usb:')) this.deviceTracks = this.deviceTracks.map((t) => (t.id === id ? fresh : t))
    else this.applyTracks([fresh])
  }

  private applyPlaylistIds(id: string, ids: string[]): void {
    this.playlists = this.playlists.map((p) => (p.id === id ? { ...p, count: ids.length } : p))
    if (this.source.type === 'playlist' && this.source.id === id) this.sourceIds = ids
  }

  private applyPlaylists(playlists: TreeItem[]): void {
    this.playlists = playlists
    const src = this.source
    if (src.type === 'playlist' && !playlists.some((p) => p.id === src.id)) void this.setSource({ type: 'collection' })
  }

  async updateTracks(ids: string[], fields: TrackEdit): Promise<boolean> {
    if (!ids.length || !Object.keys(fields).length) return true
    if (ids.some((id) => id.startsWith('file:'))) {
      this.notify('Import a file before editing it')
      return false
    }
    if (ids.some((id) => id.startsWith('usb:'))) {
      this.notify('Device tracks follow the playlist synced to that volume')
      return false
    }
    const result = await this.edit(() => api.updateTracks(ids, fields, this.writeFileTags))
    if (!result) return false
    this.applyTracks(result.tracks)
    const failed = result.tagErrors
    if (failed.length) {
      const files = failed.length === 1 ? 'one file' : `${failed.length} files`
      this.notify(`Saved to rekordbox, but couldn’t write tags to ${files}: ${failed[0]}`)
    }
    return true
  }

  async relocate(moves: Record<string, string>): Promise<boolean> {
    if (!Object.keys(moves).length) return true
    const result = await this.edit(() => api.relocate(moves))
    if (!result) return false
    dropCaches()
    this.applyTracks(result)
    return true
  }

  async locate(track: Track): Promise<void> {
    if (!this.writable) return this.notify(READ_ONLY)
    const folder = track.path.replace(/[\\/][^\\/]*$/, '')
    const path = await api.choosePath('audio', folder || undefined)
    if (path) await this.relocate({ [track.id]: path })
  }

  /** Ask for files or a folder and import them, into the open playlist when there is one. */
  async chooseImport(kind: 'files' | 'folder'): Promise<void> {
    if (!this.writable) return this.notify('Unlock editing to import tracks')
    const paths = await window.api.chooseImport(kind)
    const playlist = this.sourcePlaylist?.kind === 'playlist' ? this.sourcePlaylist.id : null
    await this.importFiles(paths, playlist)
  }

  /**
   * Import files and folders into the collection. With a playlist they are added to it too, at the
   * end or just before `beforeId` when that is given.
   */
  async importFiles(
    paths: string[],
    playlistId: string | null = null,
    beforeId?: string | null
  ): Promise<ImportResult | null> {
    if (!paths.length) return null
    if (!this.writable) {
      this.notify('Unlock editing to import tracks')
      return null
    }
    if (this.importing) {
      this.notify('Wait for the current import to finish')
      return null
    }
    this.importing = true
    try {
      const files = await window.api.expandPaths(paths)
      if (!files.length) {
        this.notify('No audio files that rekordbox can import')
        return null
      }
      const result = await this.edit(() => api.importTracks(files, playlistId, this.importArtwork))
      if (!result) return null
      const known = new Set(this.tracks.map((t) => t.id))
      const fresh = result.tracks.filter((t) => !known.has(t.id))
      if (fresh.length) this.tracks = [...this.tracks, ...fresh]
      this.applyTracks(result.tracks.filter((t) => known.has(t.id)))
      this.adoptFileImports(result.ids)
      if (playlistId && result.playlist) {
        this.applyPlaylistIds(playlistId, result.playlist)
        if (beforeId !== undefined && result.ids.length) await this.moveInPlaylist(playlistId, result.ids, beforeId)
        await this.refreshMembers()
      }
      void this.fillFileGaps()
      this.notify(this.importSummary(result, playlistId))
      if (result.tracks.length) for (const listener of this.importListeners) listener(result.tracks)
      return result
    } finally {
      this.importing = false
    }
  }

  private importSummary(result: ImportResult, playlistId: string | null): string {
    const count = (n: number): string => (n === 1 ? 'one track' : `${n} tracks`)
    const into = playlistId ? ` into ${this.playlistById.get(playlistId)?.name ?? 'the playlist'}` : ''
    const parts: string[] = []
    if (result.tracks.length) parts.push(`Imported ${count(result.tracks.length)}${into}.`)
    else if (playlistId && result.skipped) parts.push(`Added ${count(result.skipped)}${into}.`)
    if (result.skipped) {
      parts.push(
        result.skipped === 1
          ? 'One was already in the collection.'
          : `${result.skipped} were already in the collection.`
      )
    }
    if (result.failed.length) {
      const first = result.failed[0]
      const name = first.path.split(/[\\/]/).pop()
      const files = result.failed.length === 1 ? 'One file' : `${result.failed.length} files`
      parts.push(`${files} couldn’t be imported (${name}: ${first.error}).`)
    }
    return parts.join(' ') || 'Nothing to import'
  }

  /** Remove tracks from the collection and every playlist, optionally moving their files to the Trash. */
  async removeFromCollection(ids: string[], trash = false): Promise<boolean> {
    const local = ids.filter((id) => !id.startsWith('usb:'))
    if (!local.length) {
      this.notify('Device tracks follow the playlist synced to that volume')
      return false
    }
    const paths = local
      .map((id) => this.byId.get(id))
      .filter((t): t is Track => !!t && !t.missing && !t.streaming && !!t.path)
      .map((t) => t.path)
    const result = await this.edit(() => api.removeFromCollection(local))
    if (!result) return false
    const gone = new Set(result.removed)
    this.tracks = this.tracks.filter((t) => !gone.has(t.id))
    this.playlists = result.playlists
    if (this.sourceIds) this.sourceIds = this.sourceIds.filter((id) => !gone.has(id))
    this.selection = new Set([...this.selection].filter((id) => !gone.has(id)))
    if (this.selectedId && gone.has(this.selectedId)) this.selectedId = null
    for (const listener of this.removeListeners) listener(result.removed)
    await this.refreshMembers()
    void this.fillFileGaps()
    const count = gone.size === 1 ? 'one track' : `${gone.size} tracks`
    if (trash && paths.length) {
      const failed = await window.api.trash(paths)
      if (failed.length) {
        const files = failed.length === 1 ? 'one file' : `${failed.length} files`
        this.notify(`Removed ${count}, but couldn’t move ${files} to the Trash`)
        return true
      }
    }
    this.notify(`Removed ${count} from the collection`)
    return true
  }

  async addToPlaylist(playlistId: string, ids: string[]): Promise<void> {
    if (ids.some((id) => id.startsWith('file:'))) {
      this.notify('Import a file before adding it to a playlist')
      return
    }
    if (ids.some((id) => id.startsWith('usb:'))) {
      this.notify('Device tracks stay on the volume')
      return
    }
    const before = this.playlistById.get(playlistId)?.count ?? 0
    const result = await this.edit(() => api.addToPlaylist(playlistId, ids))
    if (!result) return
    this.applyPlaylistIds(playlistId, result)
    await this.refreshMembers()
    const name = this.playlistById.get(playlistId)?.name ?? 'the playlist'
    if (result.length === before) this.notify(`Already in ${name}`)
  }

  async removeFromPlaylist(playlistId: string, ids: string[]): Promise<void> {
    const result = await this.edit(() => api.removeFromPlaylist(playlistId, ids))
    if (result) {
      this.applyPlaylistIds(playlistId, result)
      await this.refreshMembers()
    }
  }

  /** Move tracks so they sit just before `beforeId`, or at the end when it is null. */
  async moveInPlaylist(playlistId: string, ids: string[], beforeId: string | null): Promise<void> {
    if (!(this.source.type === 'playlist' && this.source.id === playlistId && this.sourceIds)) return
    const current = this.sourceIds
    const found = beforeId === null ? -1 : current.indexOf(beforeId)
    const index = found < 0 ? current.length : found
    const moving = new Set(ids)
    const before = current.slice(0, index).filter((id) => !moving.has(id)).length
    const rest = current.filter((id) => !moving.has(id))
    this.sourceIds = [...rest.slice(0, before), ...current.filter((id) => moving.has(id)), ...rest.slice(before)]
    const result = await this.edit(() => api.moveInPlaylist(playlistId, ids, index))
    if (result) this.applyPlaylistIds(playlistId, result)
    else await this.setSource(this.source)
  }

  async createPlaylist(parentId = 'root', folder = false, trackIds: string[] = []): Promise<string | null> {
    const name = folder ? 'New folder' : 'New playlist'
    const result = await this.edit(() => api.createPlaylist(name, parentId, folder, trackIds))
    if (!result) return null
    this.playlists = result.playlists
    this.renamingId = result.id
    if (trackIds.length) await this.refreshMembers()
    return result.id
  }

  async renamePlaylist(id: string, name: string): Promise<void> {
    this.renamingId = null
    const current = this.playlistById.get(id)
    if (!current || !name.trim() || name.trim() === current.name) return
    this.playlists = this.playlists.map((p) => (p.id === id ? { ...p, name: name.trim() } : p))
    const result = await this.edit(() => api.renamePlaylist(id, name))
    this.applyPlaylists(result ?? this.playlists.map((p) => (p.id === id ? current : p)))
  }

  async deletePlaylist(id: string): Promise<void> {
    const result = await this.edit(() => api.deletePlaylist(id))
    if (result) {
      this.applyPlaylists(result)
      await this.refreshMembers()
    }
  }

  async setTag(tagId: string, ids: string[], on: boolean): Promise<void> {
    const result = await this.edit(() => (on ? api.addTag(tagId, ids) : api.removeTag(tagId, ids)))
    if (result) this.applyTracks(result)
  }

  private async refreshContents(): Promise<void> {
    const [tracks, playlists, histories, myTags, members] = await Promise.all([
      api.tracks(),
      api.playlists(),
      api.histories(),
      api.myTags(),
      api.playlistMembers()
    ])
    this.tracks = tracks
    this.playlists = playlists
    this.histories = histories
    this.myTags = myTags
    this.playlistMembers = new Set(members)
    this.changed = false
    await this.setSource(this.source)
  }

  async ejectDevice(volumeId: string): Promise<void> {
    await api.usbEject(volumeId)
    await this.refreshDevices()
  }

  async refreshDevices(): Promise<void> {
    try {
      const volumes = await api.usbList()
      const present = new Set(volumes.map((volume) => volume.id))
      this.devices = volumes
      const playlists = { ...this.devicePlaylists }
      for (const id of Object.keys(playlists)) {
        if (!present.has(id)) delete playlists[id]
      }
      this.devicePlaylists = playlists
      if (this.source.type === 'device' && !present.has(this.source.volumeId)) {
        await this.setSource({ type: 'collection' })
      }
    } catch (e) {
      this.notify((e as Error).message)
    }
  }

  async devicePlaylistTree(volumeId: string): Promise<TreeItem[]> {
    if (!this.devicePlaylists[volumeId]) {
      const rows = await api.usbPlaylists(volumeId)
      this.devicePlaylists = { ...this.devicePlaylists, [volumeId]: rows }
    }
    return this.devicePlaylists[volumeId]
  }

  async syncTo(volumeId: string, playlistIds: string[]): Promise<boolean> {
    if (this.syncing) return false
    if (!playlistIds.length) {
      this.syncError = 'Select a playlist to sync'
      return false
    }
    this.syncing = true
    this.syncError = null
    this.syncProgress = { phase: 'reading' }
    try {
      await api.usbSync(volumeId, playlistIds)
      const refreshPlaylists = volumeId in this.devicePlaylists
      const playlists = { ...this.devicePlaylists }
      delete playlists[volumeId]
      this.devicePlaylists = playlists
      await this.refreshDevices()
      if (refreshPlaylists) await this.devicePlaylistTree(volumeId)
      if (this.source.type === 'device' && this.source.volumeId === volumeId) await this.setSource(this.source)
      return true
    } catch (e) {
      this.syncError = (e as Error).message
      return false
    } finally {
      this.syncing = false
    }
  }

  saveView(view: View): void {
    const next = this.views.some((item) => item.id === view.id)
      ? this.views.map((item) => (item.id === view.id ? view : item))
      : [...this.views, view]
    this.views = next
    saveViews(next)
    if (this.isSource({ type: 'view', id: view.id })) void this.setSource({ type: 'view', id: view.id })
  }

  deleteView(id: string): void {
    this.views = this.views.filter((view) => view.id !== id)
    saveViews(this.views)
    if (this.isSource({ type: 'view', id })) void this.setSource({ type: 'collection' })
  }

  /** Drop files that were moved to the Trash from the open file view. */
  dropFiles(paths: string[]): void {
    const gone = new Set(paths.map((path) => pathKey(path)))
    const ids = new Set(
      this.fileEntries.flatMap((entry) => (gone.has(pathKey(entry.path)) && entry.loose ? [entry.loose.id] : []))
    )
    this.fileEntries = this.fileEntries.filter((entry) => !gone.has(pathKey(entry.path)))
    if (!ids.size) return
    this.selection = new Set([...this.selection].filter((id) => !ids.has(id)))
    if (this.selectedId && ids.has(this.selectedId)) this.selectedId = null
  }

  async setSource(source: Source): Promise<void> {
    const token = ++this.sourceToken
    if (!this.isSource(source)) {
      this.sort = null
      this.selection = this.selectedId ? new Set([this.selectedId]) : new Set()
    }
    this.source = source
    if (source.type === 'collection' || source.type === 'tag') {
      this.sourceIds = null
      this.deviceTracks = []
      this.fileEntries = []
      this.sourceLoading = false
      return
    }
    if (source.type === 'view') {
      this.sourceIds = null
      this.deviceTracks = []
      const view = this.viewById.get(source.id)
      if (!view || view.type === 'tracks') {
        this.fileEntries = []
        this.sourceLoading = false
        return
      }
      await this.loadFileView(view.directory, view.showInCollection, token)
      return
    }
    if (source.type === 'device') {
      this.sourceIds = null
      this.fileEntries = []
      this.sourceLoading = true
      try {
        const rows = await api.usbTracks(source.volumeId, source.playlistId)
        if (token === this.sourceToken) this.deviceTracks = rows
      } catch (e) {
        if (token === this.sourceToken) {
          this.deviceTracks = []
          this.notify((e as Error).message)
        }
      } finally {
        if (token === this.sourceToken) this.sourceLoading = false
      }
      return
    }
    this.deviceTracks = []
    this.fileEntries = []
    this.sourceLoading = true
    try {
      const ids = source.type === 'playlist' ? await api.playlistTracks(source.id) : await api.historyTracks(source.id)
      if (token === this.sourceToken) this.sourceIds = ids
    } finally {
      if (token === this.sourceToken) this.sourceLoading = false
    }
  }

  toggleSort(key: SortKey): void {
    const cur = this.sort
    if (!cur || cur.key !== key) this.sort = { key, dir: 1 }
    else if (cur.dir === 1) this.sort = { key, dir: -1 }
    else this.sort = null
  }

  private tracksByPath(): Map<string, Track> {
    const out = new Map<string, Track>()
    for (const track of this.tracks) {
      if (track.path) out.set(pathKey(track.path), track)
    }
    return out
  }

  private viewTracks(id: string): Track[] {
    const view = this.viewById.get(id)
    if (!view) return []
    if (view.type === 'tracks') {
      return this.tracks.filter((track) => matchesTrack(track, view.filters, this.playlistMembers.has(track.id)))
    }
    const byPath = this.tracksByPath()
    const out: Track[] = []
    for (const entry of this.fileEntries) {
      const match = byPath.get(pathKey(entry.path))
      if (match) {
        if (view.showInCollection) out.push(match)
      } else if (entry.loose) out.push(entry.loose)
    }
    return out
  }

  private async refreshMembers(): Promise<void> {
    try {
      this.playlistMembers = new Set(await api.playlistMembers())
    } catch {
      // The previous set stays. The next library refresh replaces it.
    }
  }

  /** Point a file-view selection at the collection row once that file has been imported. */
  private adoptFileImports(ids: string[]): void {
    if (!this.fileEntries.length || !ids.length) return
    const imported = new Map<string, string>()
    for (const id of ids) {
      const track = this.byId.get(id)
      if (track?.path) imported.set(pathKey(track.path), id)
    }
    const remap = new Map<string, string>()
    for (const entry of this.fileEntries) {
      const id = imported.get(pathKey(entry.path))
      if (id && entry.loose) remap.set(entry.loose.id, id)
    }
    if (!remap.size) return
    if (this.selectedId && remap.has(this.selectedId)) this.selectedId = remap.get(this.selectedId) ?? this.selectedId
    this.selection = new Set([...this.selection].map((id) => remap.get(id) ?? id))
  }

  private async loadFileView(directory: string, showInCollection: boolean, token: number): Promise<void> {
    this.sourceLoading = true
    this.fileEntries = []
    try {
      const files = await window.api.expandPaths([directory])
      if (token !== this.sourceToken) return
      const byPath = this.tracksByPath()
      const entries: FileEntry[] = []
      const pending: string[] = []
      for (const path of files) {
        const match = byPath.get(pathKey(path))
        if (match) {
          if (showInCollection) entries.push({ path, loose: null })
        } else {
          entries.push({ path, loose: null })
          pending.push(path)
        }
      }
      if (token !== this.sourceToken) return
      this.fileEntries = entries
      await this.readLoosePaths(pending, token)
    } catch (e) {
      if (token === this.sourceToken) {
        this.fileEntries = []
        this.notify((e as Error).message)
      }
    } finally {
      if (token === this.sourceToken) this.sourceLoading = false
    }
  }

  private async readLoosePaths(paths: string[], token: number): Promise<void> {
    const batch = 40
    for (let index = 0; index < paths.length; index += batch) {
      if (token !== this.sourceToken) return
      const chunk = paths.slice(index, index + batch)
      let rows: Track[]
      try {
        rows = (await api.readFiles(chunk)).map((row, i) => looseTrack(chunk[i], row))
      } catch (e) {
        this.notify((e as Error).message)
        rows = chunk.map((path) => looseTrack(path))
      }
      if (token !== this.sourceToken) return
      const byPath = new Map(rows.map((row) => [pathKey(row.path), row]))
      this.fileEntries = this.fileEntries.map((entry) => {
        const loose = byPath.get(pathKey(entry.path))
        return loose ? { ...entry, loose } : entry
      })
    }
  }

  /** Read tags for file-view paths that have left the collection. */
  private async fillFileGaps(): Promise<void> {
    if (this.source.type !== 'view') return
    const view = this.viewById.get(this.source.id)
    if (view?.type !== 'files') return
    const token = this.sourceToken
    const byPath = this.tracksByPath()
    const missing = this.fileEntries
      .filter((entry) => !entry.loose && !byPath.has(pathKey(entry.path)))
      .map((entry) => entry.path)
    if (missing.length) await this.readLoosePaths(missing, token)
  }

  isSource(source: Source): boolean {
    const cur = this.source
    if (cur.type !== source.type) return false
    if (cur.type === 'collection') return true
    if (cur.type === 'device' && source.type === 'device') {
      return cur.volumeId === source.volumeId && cur.playlistId === source.playlistId
    }
    return source.type !== 'collection' && source.type !== 'device' && 'id' in cur && cur.id === source.id
  }
}
