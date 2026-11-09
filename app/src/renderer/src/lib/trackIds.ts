const DEVICE_PREFIX = 'usb:'
const LOOSE_FILE_PREFIX = 'file:'

export const DEVICE_TRACKS_READ_ONLY = 'Device tracks are read-only; sync to change them'

export function isDeviceTrack(id: string | null | undefined): boolean {
  return !!id?.startsWith(DEVICE_PREFIX)
}

export function isLooseFile(id: string | null | undefined): boolean {
  return !!id?.startsWith(LOOSE_FILE_PREFIX)
}

export function looseFileId(path: string): string {
  return `${LOOSE_FILE_PREFIX}${encodeURIComponent(path)}`
}
