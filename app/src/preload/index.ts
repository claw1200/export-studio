import { contextBridge, ipcRenderer } from 'electron'
import type { AudioApi, DeckEvent, Tick } from './audio'

type CallResult = { ok: true; result: unknown } | { ok: false; error: { message: string; type: string } }

const api = {
  platform: process.platform,

  async call<T = unknown>(method: string, params?: unknown): Promise<T> {
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
  analyse: (trackId, bands) => ipcRenderer.invoke('wave:analyse', trackId, bands)
}

contextBridge.exposeInMainWorld('api', api)
contextBridge.exposeInMainWorld('audio', audio)
