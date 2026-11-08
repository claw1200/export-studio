import { app, ipcMain, net } from 'electron'
import { findUpdate, RELEASES_URL, type Release, type Update } from '../shared/updates'

async function checkForUpdate(): Promise<Update | null> {
  try {
    const response = await net.fetch(RELEASES_URL, {
      headers: { Accept: 'application/vnd.github+json', 'User-Agent': app.getName() }
    })
    if (!response.ok) return null
    return findUpdate(app.getVersion(), (await response.json()) as Release[])
  } catch {
    // Offline or rate-limited: say nothing rather than nag.
    return null
  }
}

export function registerUpdates(): void {
  ipcMain.handle('updates:check', checkForUpdate)
}
