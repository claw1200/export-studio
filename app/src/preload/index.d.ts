export interface Api {
  platform: string
  call<T = unknown>(method: string, params?: unknown): Promise<T>
  onEvent(listener: (event: string, data: unknown) => void): () => void
  showItemInFolder(path: string): Promise<void>
}

declare global {
  interface Window {
    api: Api
  }
}
