export type PlaylistPolicy = 'skipExisting' | 'merge' | 'createNew'
export type ConvertMode = 'copy' | 'update' | 'update_delete'

export interface SyncReviewItem {
  key: string
  entity: 'track' | 'playlist' | 'folder'
  libraryId: number
  name: string
  path: string
}

export interface SyncConflict extends SyncReviewItem {
  format: string
  deviceId: number
  artist: string
}

export interface SyncReview {
  recoveryRequired: boolean
  token: string
  conflicts: SyncConflict[]
  exclusions: SyncReviewItem[]
}

export interface SyncDecisions {
  token: string
  resolutions: Record<string, 'restore' | 'keep'>
}

export interface TrackEditFields {
  title?: string
  comment?: string
  year?: number
  trackNo?: number
  rating?: number
  color?: number
  artist?: string
  remixer?: string
  originalArtist?: string
  composer?: string
  album?: string
  genre?: string
  label?: string
}

export interface EngineCommandParams {
  'library.open': { dbPath?: string } | undefined
  'library.reload': undefined
  'library.setWritable': { writable: boolean }
  'library.info': undefined
  'library.tracks': undefined
  'library.playlists': undefined
  'library.histories': undefined
  'library.myTags': undefined
  'library.playlistMembers': undefined
  'playlist.tracks': { id: string }
  'history.tracks': { id: string }
  'track.detail': { id: string }
  'media.resolve': { kind: string; id: string; size?: string }
  'track.previews': { ids: string[] }
  'files.read': { paths: string[] }
  'track.findMissing': { ids: string[]; directory: string }
  'track.inspect': { tracks: { id: string; size?: number | string; mtime?: string | null }[] }
  'usb.list': undefined
  'usb.playlists': { volumeId: string }
  'usb.libraries': { volumeId: string }
  'usb.selection': { volumeId: string }
  'usb.tracks': { volumeId: string; playlistId?: string | null }
  'usb.detail': { id: string }
  'usb.media': { id: string }
  'usb.importPreview': { volumeId: string; folder?: string | null; policy?: PlaylistPolicy }
  'backup.list': undefined
  'backup.settings': undefined
  'backup.create': undefined
  'backup.delete': { id: string }
  'backup.restore': { id: string }
  'backup.setSettings': { auto?: boolean; keep?: number; interval?: number }
  'track.removeFromCollection': { ids: string[] }
  'track.update': { ids: string[]; fields: TrackEditFields; writeTags?: boolean }
  'track.import': { paths: string[]; playlistId?: string | null; artwork?: boolean }
  'cue.setHot': { id: string; kind: number; ms: number }
  'cue.addMemory': { id: string; ms: number }
  'cue.delete': { id: string; cueId: string }
  'cue.setColor': { id: string; cueId: string; colorIndex?: number | null }
  'playlist.add': { id: string; trackIds: string[] }
  'playlist.remove': { id: string; trackIds: string[] }
  'playlist.move': { id: string; trackIds: string[]; index: number }
  'playlist.create': { name: string; parentId?: string | null; folder: boolean; trackIds: string[] }
  'playlist.rename': { id: string; name: string }
  'playlist.delete': { id: string }
  'tag.add': { id: string; trackIds: string[] }
  'tag.remove': { id: string; trackIds: string[] }
  'beatgrid.set': {
    id: string
    times: number[]
    beats: number[]
    bpms: number[]
    cues?: { id: string; start: number; end: number | null }[]
  }
  'track.applyAnalysis': {
    id: string
    result: {
      beats?: number[][]
      waveform: string
      key?: { tonic: number; minor: boolean } | null
      bpm?: number
      duration?: number
    }
    options?: { overwrite?: boolean; grid?: boolean; key?: boolean }
  }
  'track.convert': {
    ids: string[]
    preset: { format: string; sampleRate?: number; bitDepth?: number; bitrate?: number }
    mode: ConvertMode
  }
  'track.relocate': { moves: Record<string, string> }
  'usb.eject': { volumeId: string }
  'usb.format': { volumeId: string }
  'usb.sync': { volumeId: string; playlistIds: string[]; overwrite?: boolean; review?: SyncDecisions }
  'usb.syncReview': { volumeId: string; playlistIds: string[] }
  'usb.import': { volumeId: string; folder?: string | null; policy?: PlaylistPolicy }
}

export type EngineMethod = keyof EngineCommandParams

export type EngineCallArgs = {
  [Method in EngineMethod]: undefined extends EngineCommandParams[Method]
    ? [method: Method, params?: EngineCommandParams[Method]]
    : [method: Method, params: EngineCommandParams[Method]]
}[EngineMethod]
