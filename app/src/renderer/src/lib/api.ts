import type { Cue, LibraryInfo, Track, TrackDetail, TreeItem } from './types'

const call = window.api.call

export const api = {
  open: () => call<LibraryInfo>('library.open'),
  reload: () => call<LibraryInfo>('library.reload'),
  setWritable: (writable: boolean) => call<LibraryInfo>('library.setWritable', { writable }),
  info: () => call<LibraryInfo>('library.info'),
  tracks: () => call<Track[]>('library.tracks'),
  playlists: () => call<TreeItem[]>('library.playlists'),
  histories: () => call<TreeItem[]>('library.histories'),
  myTags: () => call<TreeItem[]>('library.myTags'),
  playlistTracks: (id: string) => call<string[]>('playlist.tracks', { id }),
  historyTracks: (id: string) => call<string[]>('history.tracks', { id }),
  trackDetail: (id: string) => call<TrackDetail>('track.detail', { id }),
  setHotCue: (id: string, kind: number, ms: number) => call<Cue[]>('cue.setHot', { id, kind, ms }),
  deleteCue: (id: string, cueId: string) => call<Cue[]>('cue.delete', { id, cueId }),
  setCueColor: (id: string, cueId: string, colorIndex: number | null) =>
    call<Cue[]>('cue.setColor', { id, cueId, colorIndex }),
  onEvent: window.api.onEvent,
  showItemInFolder: window.api.showItemInFolder
}

export const artworkUrl = (id: string, size: 's' | 'm' | '' = 'm'): string =>
  `media://art/${encodeURIComponent(id)}${size ? `?size=${size}` : ''}`

export const audioUrl = (id: string): string => `media://audio/${encodeURIComponent(id)}`
