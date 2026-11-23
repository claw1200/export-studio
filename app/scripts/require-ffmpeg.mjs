// electron-builder skips a missing extraResources file silently, which ships an app that can't convert tracks.
import { existsSync } from 'node:fs'
import { join } from 'node:path'

export default function requireFfmpeg({ electronPlatformName, packager }) {
  const name = electronPlatformName === 'win32' ? 'ffmpeg.exe' : 'ffmpeg'
  const binary = join(packager.projectDir, 'resources', 'native', name)
  if (!existsSync(binary)) throw new Error(`${binary} is missing; run npm run build:ffmpeg before packaging`)
}
