import type { AnalysisJob, AnalysisProgress, MenuCommand } from './analysis'
import type { AudioApi } from './audio'
import type { EngineCallArgs } from './engine'

export interface Api {
  platform: string
  /** The operating system's Music folder. */
  musicDirectory: string
  call<T = unknown>(...args: EngineCallArgs): Promise<T>
  onEvent(listener: (event: string, data: unknown) => void): () => void
  showItemInFolder(path: string): Promise<void>
  choosePath(kind: 'audio' | 'folder', defaultPath?: string): Promise<string | null>
  /** Asks for files or folders to import, and returns the audio files they hold. */
  chooseImport(kind: 'files' | 'folder'): Promise<string[]>
  /** Expands dropped folders into the audio files under them. */
  expandPaths(paths: string[]): Promise<string[]>
  pathForFile(file: File): string
  /** Moves files to the Trash and returns the ones that could not be moved. */
  trash(paths: string[]): Promise<string[]>
  onMenu(listener: (command: MenuCommand) => void): () => void
  analyse(jobs: AnalysisJob[], concurrency: number): Promise<void>
  cancelAnalysis(): Promise<void>
  onAnalysis(listener: (progress: AnalysisProgress) => void): () => void
  /** Preferences already on disk. `exists` is false until the file has been written. */
  loadSettings(): { exists: boolean; values: Record<string, string> }
  writeSetting(key: string, value: string): void
  replaceSettings(values: Record<string, string>): void
}

declare global {
  interface Window {
    api: Api
    audio: AudioApi
  }
}
