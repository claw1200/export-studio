import type {
  Beatgrid,
  BeatgridSaved,
  Cue,
  CueMove,
  LibraryInfo,
  Track,
  TrackDetail,
  TrackEdit,
  TrackUpdate,
  TreeItem
} from './types'

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
  addMemoryCue: (id: string, ms: number) => call<Cue[]>('cue.addMemory', { id, ms }),
  deleteCue: (id: string, cueId: string) => call<Cue[]>('cue.delete', { id, cueId }),
  setCueColor: (id: string, cueId: string, colorIndex: number | null) =>
    call<Cue[]>('cue.setColor', { id, cueId, colorIndex }),
  setBeatgrid: (id: string, grid: Beatgrid, cues: CueMove[]) =>
    call<BeatgridSaved>('beatgrid.set', { id, times: grid.times, beats: grid.beats, bpms: grid.bpms, cues }),
  updateTracks: (ids: string[], fields: TrackEdit, writeTags: boolean) =>
    call<TrackUpdate>('track.update', { ids, fields, writeTags }),
  addToPlaylist: (id: string, trackIds: string[]) => call<string[]>('playlist.add', { id, trackIds }),
  removeFromPlaylist: (id: string, trackIds: string[]) => call<string[]>('playlist.remove', { id, trackIds }),
  moveInPlaylist: (id: string, trackIds: string[], index: number) =>
    call<string[]>('playlist.move', { id, trackIds, index }),
  createPlaylist: (name: string, parentId: string, folder: boolean, trackIds: string[]) =>
    call<{ id: string; playlists: TreeItem[] }>('playlist.create', { name, parentId, folder, trackIds }),
  renamePlaylist: (id: string, name: string) => call<TreeItem[]>('playlist.rename', { id, name }),
  deletePlaylist: (id: string) => call<TreeItem[]>('playlist.delete', { id }),
  addTag: (id: string, trackIds: string[]) => call<Track[]>('tag.add', { id, trackIds }),
  removeTag: (id: string, trackIds: string[]) => call<Track[]>('tag.remove', { id, trackIds }),
  onEvent: window.api.onEvent,
  showItemInFolder: window.api.showItemInFolder
}

function mediaUrl(kind: 'art', id: string, query = ''): string {
  return `media://${kind}/${encodeURIComponent(id)}${query}`
}

export const artworkUrl = (id: string, size: 's' | 'm' | '' = 'm'): string =>
  mediaUrl('art', id, size ? `?size=${size}` : '')
