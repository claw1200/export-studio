export type MenuCommand = 'import.files' | 'import.folder' | 'settings'

export type GridMode = 'normal' | 'dynamic'

export interface AnalysisOptions {
  /** Detect tempo and the beatgrid. */
  grid: boolean
  /** Detect the musical key. */
  key: boolean
  gridMode: GridMode
  minBpm: number
  maxBpm: number
  /** Place beats on kick attacks rather than onset envelope peaks. */
  highPrecision: boolean
  /** Snap the tempo to a whole number when it is within 0.05 BPM of one. */
  roundBpm: boolean
  /** Replace a grid the track already has, instead of keeping it. */
  overwrite: boolean
}

export interface AnalysisJob {
  id: string
  options: AnalysisOptions
}

export interface AnalysisFinished {
  id: string
  /** The track row after its analysis was saved. */
  track?: unknown
  error?: string
}

export interface AnalysisProgress {
  total: number
  done: number
  failed: number
  active: string[]
  finished?: AnalysisFinished
}
