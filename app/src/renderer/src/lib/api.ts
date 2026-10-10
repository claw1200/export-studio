import type {
  Backup,
  BackupSettings,
  Beatgrid,
  BeatgridSaved,
  Cue,
  CueMove,
  ImportResult,
  LibraryInfo,
  MissingScan,
  RemoveResult,
  RestoreResult,
  Track,
  TrackDetail,
  TrackPreview,
  TrackEdit,
  TrackUpdate,
  TreeItem,
  Volume
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
  playlistMembers: () => call<string[]>('library.playlistMembers'),
  readFiles: (paths: string[]) => call<Partial<Track>[]>('files.read', { paths }),
  playlistTracks: (id: string) => call<string[]>('playlist.tracks', { id }),
  historyTracks: (id: string) => call<string[]>('history.tracks', { id }),
  trackDetail: (id: string) => call<TrackDetail>('track.detail', { id }),
  trackPreviews: (ids: string[]) => call<TrackPreview[]>('track.previews', { ids }),
  setHotCue: (id: string, kind: number, ms: number) => call<Cue[]>('cue.setHot', { id, kind, ms }),
  addMemoryCue: (id: string, ms: number) => call<Cue[]>('cue.addMemory', { id, ms }),
  deleteCue: (id: string, cueId: string) => call<Cue[]>('cue.delete', { id, cueId }),
  setCueColor: (id: string, cueId: string, colorIndex: number | null) =>
    call<Cue[]>('cue.setColor', { id, cueId, colorIndex }),
  setBeatgrid: (id: string, grid: Beatgrid, cues: CueMove[]) =>
    call<BeatgridSaved>('beatgrid.set', { id, times: grid.times, beats: grid.beats, bpms: grid.bpms, cues }),
  updateTracks: (ids: string[], fields: TrackEdit, writeTags: boolean) =>
    call<TrackUpdate>('track.update', { ids, fields, writeTags }),
  relocate: (moves: Record<string, string>) => call<Track[]>('track.relocate', { moves }),
  findMissing: (ids: string[], directory: string) => call<MissingScan>('track.findMissing', { ids, directory }),
  importTracks: (paths: string[], playlistId: string | null, artwork: boolean) =>
    call<ImportResult>('track.import', { paths, playlistId, artwork }),
  removeFromCollection: (ids: string[]) => call<RemoveResult>('track.removeFromCollection', { ids }),
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
  usbList: () => call<Volume[]>('usb.list'),
  usbEject: (volumeId: string) => call<void>('usb.eject', { volumeId }),
  usbPlaylists: (volumeId: string) => call<TreeItem[]>('usb.playlists', { volumeId }),
  usbTracks: (volumeId: string, playlistId: string | null) => call<Track[]>('usb.tracks', { volumeId, playlistId }),
  usbSync: (volumeId: string, playlistIds: string[]) =>
    call<{ volumeId: string; tracks: number; playlists: number }>('usb.sync', { volumeId, playlistIds }),
  backupList: () => call<Backup[]>('backup.list'),
  backupCreate: () => call<Backup>('backup.create'),
  backupRestore: (id: string) => call<RestoreResult>('backup.restore', { id }),
  backupDelete: (id: string) => call<Backup[]>('backup.delete', { id }),
  backupSettings: () => call<BackupSettings>('backup.settings'),
  setBackupSettings: (settings: Partial<BackupSettings>) => call<BackupSettings>('backup.setSettings', settings),
  onEvent: window.api.onEvent,
  showItemInFolder: window.api.showItemInFolder,
  choosePath: window.api.choosePath
}

function mediaUrl(kind: 'art', id: string, query = ''): string {
  return `media://${kind}/${encodeURIComponent(id)}${query}`
}

export const artworkUrl = (id: string, size: 's' | 'm' | '' = 'm'): string =>
  mediaUrl('art', id, size ? `?size=${size}` : '')
