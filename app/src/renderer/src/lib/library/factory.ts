/**
 * Rekordbox installs stock sampler audio under Music/rekordbox/Sampler.
 * Factory banks live in a PRESET folder (Groove Circuit kits, OSC one-shots)
 * or in MERGE FX. Captures and recordings sit beside those and stay visible.
 */
export function isRekordboxPresetTrack(path: string): boolean {
  const stored = path.replaceAll('\\', '/').toLowerCase()
  const marker = '/rekordbox/sampler/'
  const at = stored.indexOf(marker)
  if (at < 0) return false
  const folders = stored.slice(at + marker.length).split('/')
  folders.pop()
  return folders.some((folder) => folder === 'merge fx' || folder.startsWith('preset'))
}
