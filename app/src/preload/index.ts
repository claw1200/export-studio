import { contextBridge, ipcRenderer } from 'electron'

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

contextBridge.exposeInMainWorld('api', api)
