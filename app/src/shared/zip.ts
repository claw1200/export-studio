/** A file to put in an archive, under the folders named by `folders` (outermost first). */
export interface ZipSource {
  path: string
  folders: string[]
}

export interface ZipProgress {
  done: number
  total: number
}

export interface ZipResult {
  path: string
  written: number
  skipped: number
}
