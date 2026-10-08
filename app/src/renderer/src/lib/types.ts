export interface Track {
  id: string
  title: string
  artist: string
  album: string
  genre: string
  label: string
  remixer: string
  originalArtist: string
  composer: string
  key: string
  bpm: number
  duration: number
  rating: number
  color: number
  comment: string
  year: number
  dateAdded: string
  dateCreated: string
  playCount: number
  bitrate: number
  sampleRate: number
  bitDepth: number
  fileType: string
  path: string
  fileSize: number
  hasArtwork: boolean
  trackNo: number
  analysed: boolean
  myTags: string[]
}

export type EditableField =
  | 'title'
  | 'artist'
  | 'album'
  | 'genre'
  | 'label'
  | 'remixer'
  | 'originalArtist'
  | 'composer'
  | 'comment'
  | 'year'
  | 'trackNo'
  | 'rating'
  | 'color'

export type TrackEdit = Partial<Pick<Track, EditableField>>

export interface TrackUpdate {
  tracks: Track[]
  tagErrors: string[]
}

export interface TreeItem {
  id: string
  name: string
  parentId: string
  seq: number
  kind: 'playlist' | 'folder' | 'smart' | 'tag'
  count?: number | null
  date?: string
}

export interface Cue {
  id: string
  kind: 'hot' | 'memory'
  slot: number
  start: number
  end: number | null
  color: number | null
  colorIndex: number | null
  comment: string
  rgb?: string | null
}

export interface Beatgrid {
  times: number[]
  beats: number[]
  bpms: number[]
}

export interface CueMove {
  id: string
  start: number
  end: number | null
}

export interface BeatgridSaved {
  beatgrid: Beatgrid | null
  cues: Cue[]
  bpm: number | null
}

export interface WaveformPayload {
  rate: number
  rgb: { length: number; heights: string; colors: string } | null
  threeBand: { length: number; bands: string } | null
  threeBandPreview: { length: number; bands: string } | null
  mono: { length: number; heights: string; whiteness: string } | null
}

export interface TrackDetail {
  id: string
  fileExists: boolean
  cues: Cue[]
  beatgrid: Beatgrid | null
  waveform: WaveformPayload
}

export interface LibraryInfo {
  dbPath: string
  snapshotTime: number
  rekordboxRunning: boolean
  sourceChanged: boolean
  writable: boolean
}

export interface Volume {
  id: string
  name: string
  path: string
  hasLibrary: boolean
  rootName: string | null
}

export interface SyncProgress {
  phase: string
  done?: number
  total?: number
  name?: string
}

export type Source =
  | { type: 'collection' }
  | { type: 'playlist'; id: string }
  | { type: 'history'; id: string }
  | { type: 'tag'; id: string }
  | { type: 'device'; volumeId: string; playlistId: string | null }
