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

export interface WaveformPayload {
  rate: number
  rgb: { length: number; heights: string; colors: string } | null
  threeBand: { length: number; bands: string } | null
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
}

export type Source =
  | { type: 'collection' }
  | { type: 'playlist'; id: string }
  | { type: 'history'; id: string }
  | { type: 'tag'; id: string }
