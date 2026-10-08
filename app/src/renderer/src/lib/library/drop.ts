/** Whether a drag carries files from outside the app, such as from Finder or Explorer. */
export function isFileDrag(e: DragEvent): boolean {
  return !!e.dataTransfer?.types.includes('Files')
}

/** The paths of the files and folders in a drop. */
export function droppedPaths(e: DragEvent): string[] {
  const files = Array.from(e.dataTransfer?.files ?? [])
  return files.map((file) => window.api.pathForFile(file)).filter(Boolean)
}
