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
  /** A local file that isn't on disk at `path`. Streaming tracks are never missing. */
  missing: boolean
  streaming: boolean
  fileSize: number
  hasArtwork: boolean
  trackNo: number
  analysed: boolean
  myTags: string[]
  /** Memory cues, hot cues and phrase analysis: one star each, so 0–3. */
  cueStars: number
  /** Memory cues and hot cues placed on the track. Loops are not counted. */
  cueCount: number
  /** Phrase analysis is present, which is one of the CUE stars. */
  phrase: boolean
  /** False for a file-list row that is not in the collection. Absent means it is. */
  collected?: boolean
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

export interface MissingScan {
  matches: { id: string; path: string | null }[]
  scanned: number
}

export interface ImportResult {
  /** Rows for tracks that are new to the collection, or came back to it. */
  tracks: Track[]
  /** Every imported or already present track, in the order given. */
  ids: string[]
  skipped: number
  failed: { path: string; error: string }[]
  /** The playlist's tracks after the import, when one was given. */
  playlist: string[] | null
}

export interface RemoveResult {
  removed: string[]
  playlists: TreeItem[]
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

export interface TrackPreview {
  id: string
  memory: boolean
  hot: boolean
  phrase: boolean
  cues: Cue[]
  waveform: {
    bandLength?: number
    bands?: string
    rgbLength?: number
    heights?: string
    colors?: string
  } | null
}

export interface TrackDetail {
  id: string
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
  ejectable: boolean
  rootName: string | null
}

export interface SyncProgress {
  phase: string
  done?: number
  total?: number
  name?: string
}

export type BackupReason = 'manual' | 'before-edit' | 'daily' | 'pre-restore'

export interface Backup {
  id: string
  path: string
  reason: BackupReason
  created: number
  createdAt: string
  dbPath: string
  /** Made from a different rekordbox library than the one open now. */
  otherLibrary: boolean
  /** False for the copy kept of a library that was replaced by a restore; it was never checked. */
  verified: boolean
  tracks?: number
  playlists?: number
  files: number
  bytes: number
  size: number
}

export interface BackupSettings {
  auto: boolean
  keep: number
}

export interface BackupProgress {
  stage: 'checking' | 'saving' | 'extracting' | 'done'
  reason: BackupReason | 'restore'
  done?: number
  total?: number
}

export interface RestoreResult {
  restored: string
  kept: string | null
  tracks: number
  playlists: number
}

/** Whether a track-list view keeps tracks that sit in a normal playlist. */
export type PlaylistMembership = 'any' | 'in' | 'out'

/** Fields that must agree before two tracks count as the same recording. */
export interface DuplicateMatch {
  title: boolean
  artist: boolean
  /** Whole seconds, compared within the view's duration tolerance. */
  duration: boolean
  /** Exact byte size. Tracks with an unknown size are left out. */
  fileSize: boolean
}

/** Whole seconds of duration tolerance. 0 means the durations must be equal. */
export type DurationSlack = 0 | 1 | 2 | 5

export interface TrackViewFilters {
  title: string
  artist: string
  album: string
  genre: string
  label: string
  comment: string
  bpmMin: number | null
  bpmMax: number | null
  /** Seconds. Null means that side of the range is open. */
  durationMin: number | null
  durationMax: number | null
  /** Empty means any key. A track matches when its key is one of these. */
  keys: string[]
  playlist: PlaylistMembership
}

export type View =
  | { id: string; name: string; type: 'tracks'; filters: TrackViewFilters }
  | { id: string; name: string; type: 'files'; directory: string; showInCollection: boolean }
  | { id: string; name: string; type: 'duplicates'; match: DuplicateMatch; durationSlack: DurationSlack }

export type Source =
  | { type: 'collection' }
  | { type: 'playlist'; id: string }
  | { type: 'history'; id: string }
  | { type: 'tag'; id: string }
  | { type: 'device'; volumeId: string; playlistId: string | null }
  | { type: 'view'; id: string }
