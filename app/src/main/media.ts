import { extname } from 'path'
import { pathToFileURL } from 'url'
import { net, protocol } from 'electron'
import type { Engine } from './engine'

const MIME: Record<string, string> = {
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.png': 'image/png',
  '.mp3': 'audio/mpeg',
  '.m4a': 'audio/mp4',
  '.aac': 'audio/aac',
  '.flac': 'audio/flac',
  '.wav': 'audio/wav',
  '.aif': 'audio/aiff',
  '.aiff': 'audio/aiff',
  '.ogg': 'audio/ogg'
}

export function registerMediaScheme(): void {
  protocol.registerSchemesAsPrivileged([
    { scheme: 'media', privileges: { standard: true, secure: true, stream: true, supportFetchAPI: true } }
  ])
}

/**
 * media://art/<trackId>?size=s|m  and  media://audio/<trackId>
 * Paths are resolved by the engine, so the renderer can only reach files the library references.
 * Chromium's file loader owns range reads, cancellation, and end-of-file.
 */
export function handleMediaProtocol(engine: Engine): void {
  const cache = new Map<string, string | null>()
  engine.onEvent((event) => {
    if (event === 'library.reloaded') cache.clear()
  })

  protocol.handle('media', async (request) => {
    const url = new URL(request.url)
    const kind = url.host
    const id = decodeURIComponent(url.pathname.slice(1))
    const size = url.searchParams.get('size') ?? ''
    const key = `${kind}/${id}/${size}`
    try {
      let path = cache.get(key)
      if (path === undefined) {
        path = await engine.call<string | null>('media.resolve', { kind, id, size })
        cache.set(key, path)
      }
      if (!path) return new Response(null, { status: 404 })

      const headers = new Headers()
      const range = request.headers.get('range')
      if (range) headers.set('range', range)
      const file = await net.fetch(pathToFileURL(path).href, { headers, signal: request.signal })
      const type = MIME[extname(path).toLowerCase()]
      if (!type) return file
      const out = new Headers(file.headers)
      out.set('content-type', type)
      return new Response(file.body, { status: file.status, statusText: file.statusText, headers: out })
    } catch (err) {
      if (request.signal.aborted || (err as Error).name === 'AbortError') return new Response(null, { status: 499 })
      console.error('[media]', key, err)
      return new Response(null, { status: 404 })
    }
  })
}
