import { app, shell, BrowserWindow, dialog, ipcMain, nativeTheme, type OpenDialogOptions } from 'electron'
import { join } from 'path'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'
import icon from '../../resources/icon.png?asset'
import macIcon from '../../resources/icon-mac.png?asset'
import { AnalysisQueue } from './analysis'
import { AudioBridge } from './audio/bridge'
import { buildMenu, registerFiles } from './files'
import { handleMediaProtocol, registerMediaScheme } from './media'
import { ffmpegBinary, Library, usbExport } from './library'
import { flushSettings, loadSettings, registerSettings } from './settings'
import type { EngineCallArgs } from '../preload/engine'

const AUDIO_EXTENSIONS = ['mp3', 'm4a', 'aac', 'mp4', 'flac', 'wav', 'aif', 'aiff', 'ogg', 'alac']

process.env.EXPORTSTUDIO_USB_EXPORT ??= usbExport()
process.env.EXPORTSTUDIO_FFMPEG ??= ffmpegBinary()
const library = new Library(() => audio.addon())
const audio = new AudioBridge(library)
const analysis = new AnalysisQueue(library, () => audio.addon())
registerMediaScheme()

function createWindow(): BrowserWindow {
  const isMac = process.platform === 'darwin'
  const win = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 960,
    minHeight: 600,
    show: false,
    backgroundColor: '#0b0c0f',
    autoHideMenuBar: true,
    titleBarStyle: 'hidden',
    ...(isMac
      ? { trafficLightPosition: { x: 14, y: 14 } }
      : { titleBarOverlay: { color: '#0b0c0f', symbolColor: '#a1a1aa', height: 40 } }),
    ...(process.platform === 'darwin' ? {} : { icon }),
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      sandbox: false,
      contextIsolation: true
    }
  })

  win.on('ready-to-show', () => win.show())
  win.webContents.setWindowOpenHandler((details) => {
    shell.openExternal(details.url)
    return { action: 'deny' }
  })

  const forward = (event: string, data: unknown): void => {
    if (!win.isDestroyed()) win.webContents.send('engine:event', event, data)
  }
  const unsubscribe = library.onEvent(forward)
  win.on('closed', unsubscribe)

  if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
    win.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    win.loadFile(join(__dirname, '../renderer/index.html'))
  }
  return win
}

app.whenReady().then(() => {
  electronApp.setAppUserModelId('com.exportstudio.app')
  nativeTheme.themeSource = 'dark'
  if (process.platform === 'darwin' && is.dev) app.dock?.setIcon(macIcon)

  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window, { zoom: true })
  })

  library.start()
  handleMediaProtocol(library)
  audio.register()
  analysis.register()
  buildMenu()
  registerFiles()
  loadSettings()
  registerSettings()

  ipcMain.handle('engine:call', async (_e, ...args: EngineCallArgs) => {
    try {
      const [method, params] = args
      return { ok: true, result: await library.call(method, params) }
    } catch (err) {
      const e = err as Error & { type?: string }
      return { ok: false, error: { message: e.message, type: e.type ?? 'Error' } }
    }
  })
  ipcMain.handle('shell:showItem', (_e, path: string) => shell.showItemInFolder(path))
  ipcMain.on('app:music', (event) => {
    event.returnValue = app.getPath('music').replaceAll('\\', '/')
  })
  ipcMain.handle('dialog:choose', async (e, kind: 'audio' | 'folder', defaultPath?: string) => {
    const options: OpenDialogOptions =
      kind === 'audio'
        ? {
            title: 'Locate audio file',
            properties: ['openFile'],
            filters: [{ name: 'Audio', extensions: AUDIO_EXTENSIONS }]
          }
        : { title: 'Choose a folder to search', properties: ['openDirectory'] }
    if (defaultPath) options.defaultPath = defaultPath
    const win = BrowserWindow.fromWebContents(e.sender)
    const result = win ? await dialog.showOpenDialog(win, options) : await dialog.showOpenDialog(options)
    return result.canceled ? null : (result.filePaths[0] ?? null)
  })

  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  audio.stop()
  if (process.platform !== 'darwin') app.quit()
})

app.on('will-quit', () => {
  flushSettings()
  audio.stop()
  library.stop()
})
