import { BrowserWindow, dialog, ipcMain, Menu, shell, type MenuItemConstructorOptions } from 'electron'
import { readdir, stat } from 'fs/promises'
import { extname, join } from 'path'
import type { MenuCommand } from '../preload/analysis'

/** The file types rekordbox can hold in its collection. */
export const IMPORT_EXTENSIONS = ['mp3', 'm4a', 'flac', 'wav', 'aif', 'aiff']
const MAX_DEPTH = 16
const MAX_FILES = 100_000

function importable(path: string): boolean {
  return IMPORT_EXTENSIONS.includes(extname(path).slice(1).toLowerCase())
}

/** Expands folders into the audio files under them, in name order, skipping hidden folders. */
export async function expandPaths(paths: string[]): Promise<string[]> {
  const out: string[] = []
  const seen = new Set<string>()
  const add = (path: string): void => {
    if (!seen.has(path) && out.length < MAX_FILES) {
      seen.add(path)
      out.push(path)
    }
  }
  const walk = async (folder: string, depth: number): Promise<void> => {
    if (depth > MAX_DEPTH || out.length >= MAX_FILES) return
    let entries
    try {
      entries = await readdir(folder, { withFileTypes: true })
    } catch {
      return
    }
    entries.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }))
    for (const entry of entries) {
      if (entry.name.startsWith('.')) continue
      const path = join(folder, entry.name)
      if (entry.isDirectory()) await walk(path, depth + 1)
      else if (entry.isFile() && importable(path)) add(path)
    }
  }
  for (const path of paths) {
    const info = await stat(path).catch(() => null)
    if (info?.isDirectory()) await walk(path, 0)
    else if (info?.isFile()) add(path)
  }
  return out
}

function send(command: MenuCommand): void {
  const win = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0]
  win?.webContents.send('menu:command', command)
}

export function buildMenu(): void {
  const isMac = process.platform === 'darwin'
  const file: MenuItemConstructorOptions = {
    label: 'File',
    submenu: [
      { label: 'Import Tracks…', click: () => send('import.files') },
      { label: 'Import Folder…', click: () => send('import.folder') },
      ...(isMac ? [] : [{ type: 'separator' as const }, { label: 'Settings…', click: () => send('settings') }]),
      { type: 'separator' },
      isMac ? { role: 'close' } : { role: 'quit' }
    ]
  }
  const template: MenuItemConstructorOptions[] = [
    ...(isMac
      ? [
          {
            role: 'appMenu' as const,
            submenu: [
              { role: 'about' as const },
              { type: 'separator' as const },
              { label: 'Settings…', click: () => send('settings') },
              { type: 'separator' as const },
              { role: 'services' as const },
              { type: 'separator' as const },
              { role: 'hide' as const },
              { role: 'hideOthers' as const },
              { role: 'unhide' as const },
              { type: 'separator' as const },
              { role: 'quit' as const }
            ]
          }
        ]
      : []),
    file,
    { role: 'editMenu' },
    {
      label: 'View',
      submenu: [
        { role: 'resetZoom' },
        { role: 'zoomIn' },
        { role: 'zoomOut' },
        { type: 'separator' },
        { role: 'togglefullscreen' }
      ]
    },
    { role: 'windowMenu' }
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

export function registerFiles(): void {
  ipcMain.handle('dialog:import', async (e, kind: 'files' | 'folder') => {
    const win = BrowserWindow.fromWebContents(e.sender)
    const options: Electron.OpenDialogOptions =
      kind === 'files'
        ? {
            title: 'Import tracks',
            properties: ['openFile', 'multiSelections'],
            filters: [{ name: 'Audio', extensions: IMPORT_EXTENSIONS }]
          }
        : { title: 'Import a folder', properties: ['openDirectory', 'multiSelections'] }
    const result = win ? await dialog.showOpenDialog(win, options) : await dialog.showOpenDialog(options)
    return result.canceled ? [] : expandPaths(result.filePaths)
  })
  ipcMain.handle('files:expand', (_e, paths: string[]) => expandPaths(paths))
  ipcMain.handle('shell:trash', async (_e, paths: string[]) => {
    const failed: string[] = []
    for (const path of paths) {
      try {
        await shell.trashItem(path)
      } catch {
        failed.push(path)
      }
    }
    return failed
  })
}
