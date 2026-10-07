import { api } from './api'
import { clearDetailCache } from './player.svelte'
import type { LibraryInfo, Source, Track, TreeItem } from './types'

export type SortKey =
  | 'title'
  | 'artist'
  | 'album'
  | 'genre'
  | 'label'
  | 'bpm'
  | 'key'
  | 'duration'
  | 'rating'
  | 'color'
  | 'dateAdded'
  | 'playCount'
  | 'year'
  | 'bitrate'
  | 'fileType'
  | 'comment'
  | 'remixer'

export interface TreeNode extends TreeItem {
  children: TreeNode[]
}

export function buildTree(items: TreeItem[]): TreeNode[] {
  const nodes = new Map<string, TreeNode>(items.map((i) => [i.id, { ...i, children: [] }]))
  const roots: TreeNode[] = []
  for (const node of nodes.values()) {
    const parent = nodes.get(node.parentId)
    if (parent) parent.children.push(node)
    else roots.push(node)
  }
  const sort = (list: TreeNode[]): TreeNode[] => {
    list.sort((a, b) => a.seq - b.seq)
    for (const n of list) sort(n.children)
    return list
  }
  return sort(roots)
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

function camelotRank(key: string): number {
  const m = /^(\d{1,2})([AB])$/i.exec(key)
  if (!m) return key ? 1000 : 2000
  return Number(m[1]) * 2 + (m[2].toUpperCase() === 'B' ? 1 : 0)
}

function compare(a: Track, b: Track, key: SortKey): number {
  switch (key) {
    case 'bpm':
    case 'duration':
    case 'rating':
    case 'playCount':
    case 'year':
    case 'bitrate':
    case 'color':
      return a[key] - b[key]
    case 'key':
      return camelotRank(a.key) - camelotRank(b.key)
    default: {
      const av = a[key]
      const bv = b[key]
      if (!av && bv) return 1
      if (av && !bv) return -1
      return collator.compare(av, bv)
    }
  }
}

export class Library {
  tracks = $state.raw<Track[]>([])
  playlists = $state.raw<TreeItem[]>([])
  histories = $state.raw<TreeItem[]>([])
  myTags = $state.raw<TreeItem[]>([])
  info = $state.raw<LibraryInfo | null>(null)

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
  sort = $state.raw<{ key: SortKey; dir: 1 | -1 } | null>(null)
  selectedId = $state<string | null>(null)

  byId = $derived(new Map(this.tracks.map((t) => [t.id, t])))
  playlistTree = $derived(buildTree(this.playlists))
  historyTree = $derived(buildTree(this.histories))
  tagTree = $derived(buildTree(this.myTags))
  tagNames = $derived(new Map(this.myTags.map((t) => [t.id, t.name])))
  tagCounts = $derived.by(() => {
    const counts = new Map<string, number>()
    for (const t of this.tracks) for (const id of t.myTags) counts.set(id, (counts.get(id) ?? 0) + 1)
    return counts
  })

  private haystack = $derived(
    new Map(
      this.tracks.map((t) => [
        t.id,
        [t.title, t.artist, t.album, t.genre, t.label, t.remixer, t.comment, t.key, t.originalArtist]
          .join('\u0001')
          .toLowerCase()
      ])
    )
  )

  sourceTracks = $derived.by(() => {
    const src = this.source
    if (src.type === 'collection') return this.tracks
    if (src.type === 'tag') return this.tracks.filter((t) => t.myTags.includes(src.id))
    if (!this.sourceIds) return []
    const out: Track[] = []
    for (const id of this.sourceIds) {
      const t = this.byId.get(id)
      if (t) out.push(t)
    }
    return out
  })

  visible = $derived.by(() => {
    let list = this.sourceTracks
    const terms = this.query.toLowerCase().split(/\s+/).filter(Boolean)
    if (terms.length) {
      list = list.filter((t) => {
        const h = this.haystack.get(t.id) ?? ''
        return terms.every((term) => h.includes(term))
      })
    }
    const sort = this.sort
    if (sort) {
      list = [...list].sort((a, b) => compare(a, b, sort.key) * sort.dir || collator.compare(a.title, b.title))
    } else if (this.source.type === 'collection' || this.source.type === 'tag') {
      list = [...list].sort((a, b) => collator.compare(b.dateAdded, a.dateAdded))
    }
    return list
  })

  selected = $derived(this.selectedId ? (this.byId.get(this.selectedId) ?? null) : null)

  sourceTitle = $derived.by(() => {
    const src = this.source
    if (src.type === 'collection') return 'Collection'
    const list = src.type === 'playlist' ? this.playlists : src.type === 'history' ? this.histories : this.myTags
    return list.find((i) => i.id === src.id)?.name ?? ''
  })

  private sourceToken = 0

  constructor() {
    api.onEvent((event, data) => {
      if (event === 'library.changed') this.changed = true
      if (event === 'library.rekordbox') {
        const running = Boolean((data as { running?: boolean } | undefined)?.running)
        if (this.info && this.info.rekordboxRunning !== running) {
          this.info = { ...this.info, rekordboxRunning: running }
        }
        if (!running) this.lockError = null
      }
      if (event === 'library.locked') {
        clearDetailCache()
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
    if (reload) clearDetailCache()
    try {
      this.info = reload ? await api.reload() : await api.open()
      await this.refreshContents()
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
      clearDetailCache()
      await this.refreshContents()
      return this.writable === on
    } catch (e) {
      this.lockError = (e as Error).message
      try {
        const info = await api.info()
        const reopened = info.writable !== this.writable || info.snapshotTime !== this.info?.snapshotTime
        this.info = info
        if (reopened) {
          clearDetailCache()
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

  private async refreshContents(): Promise<void> {
    const [tracks, playlists, histories, myTags] = await Promise.all([
      api.tracks(),
      api.playlists(),
      api.histories(),
      api.myTags()
    ])
    this.tracks = tracks
    this.playlists = playlists
    this.histories = histories
    this.myTags = myTags
    this.changed = false
    await this.setSource(this.source)
  }

  async setSource(source: Source): Promise<void> {
    const token = ++this.sourceToken
    this.source = source
    this.sort = null
    if (source.type === 'collection' || source.type === 'tag') {
      this.sourceIds = null
      return
    }
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

  isSource(source: Source): boolean {
    const cur = this.source
    if (cur.type !== source.type) return false
    return cur.type === 'collection' || (source.type !== 'collection' && 'id' in cur && cur.id === source.id)
  }
}
