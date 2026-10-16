import { contextBridge, ipcRenderer, webUtils } from 'electron'
import type { AudioApi, DeckEvent, Tick } from './audio'
import type { AnalysisJob, AnalysisProgress, MenuCommand } from './analysis'
import type { EngineCallArgs } from './engine'

type CallResult = { ok: true; result: unknown } | { ok: false; error: { message: string; type: string } }

const api = {
  platform: process.platform,
  musicDirectory: ipcRenderer.sendSync('app:music') as string,

  async call<T = unknown>(...args: EngineCallArgs): Promise<T> {
    const [method, params] = args
    const res = (await ipcRenderer.invoke('engine:call', method, params)) as CallResult
    if (!res.ok) throw Object.assign(new Error(res.error.message), { type: res.error.type })
    return res.result as T
  },

  onEvent(listener: (event: string, data: unknown) => void): () => void {
    const handler = (_: Electron.IpcRendererEvent, event: string, data: unknown): void => listener(event, data)
    ipcRenderer.on('engine:event', handler)
    return () => ipcRenderer.removeListener('engine:event', handler)
  },

  showItemInFolder(path: string): Promise<void> {
    return ipcRenderer.invoke('shell:showItem', path)
  },

  choosePath(kind: 'audio' | 'folder', defaultPath?: string): Promise<string | null> {
    return ipcRenderer.invoke('dialog:choose', kind, defaultPath)
  },

  chooseImport(kind: 'files' | 'folder'): Promise<string[]> {
    return ipcRenderer.invoke('dialog:import', kind)
  },

  expandPaths(paths: string[]): Promise<string[]> {
    return ipcRenderer.invoke('files:expand', paths)
  },

  pathForFile(file: File): string {
    return webUtils.getPathForFile(file)
  },

  trash(paths: string[]): Promise<string[]> {
    return ipcRenderer.invoke('shell:trash', paths)
  },

  onMenu(listener: (command: MenuCommand) => void): () => void {
    return subscribe<MenuCommand>('menu:command', listener)
  },

  analyse(jobs: AnalysisJob[], concurrency: number): Promise<void> {
    return ipcRenderer.invoke('analysis:enqueue', jobs, concurrency)
  },

  cancelAnalysis(): Promise<void> {
    return ipcRenderer.invoke('analysis:cancel')
  },

  onAnalysis(listener: (progress: AnalysisProgress) => void): () => void {
    return subscribe<AnalysisProgress>('analysis:progress', listener)
  },

  loadSettings(): { exists: boolean; values: Record<string, string> } {
    return ipcRenderer.sendSync('settings:load')
  },

  writeSetting(key: string, value: string): void {
    ipcRenderer.send('settings:set', key, value)
  },

  replaceSettings(values: Record<string, string>): void {
    ipcRenderer.send('settings:replace', values)
  }
}

function subscribe<T>(channel: string, listener: (value: T) => void): () => void {
  const handler = (_: Electron.IpcRendererEvent, value: T): void => listener(value)
  ipcRenderer.on(channel, handler)
  return () => ipcRenderer.removeListener(channel, handler)
}

let sequence = 0

const audio: AudioApi = {
  status: () => ipcRenderer.invoke('audio:status'),
  send: (command) => {
    ipcRenderer.send('deck:cmd', command, ++sequence)
    return sequence
  },
  onTick: (listener) => subscribe<Tick>('deck:tick', listener),
  onEvent: (listener) => subscribe<DeckEvent>('deck:event', listener),
  analyse: (trackId, bands, quality = 100) => ipcRenderer.invoke('wave:analyse', trackId, bands, quality)
}

contextBridge.exposeInMainWorld('api', api)
contextBridge.exposeInMainWorld('audio', audio)
