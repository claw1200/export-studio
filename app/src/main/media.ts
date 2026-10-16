import { createReadStream } from 'fs'
import { stat } from 'fs/promises'
import { extname } from 'path'
import { Readable } from 'stream'
import { protocol } from 'electron'
import type { Library } from './library'

/** What the media protocol needs from the library: resolving files and hearing when the library reloads. */
type Caller = Pick<Library, 'call' | 'onEvent'>

const MIME: Record<string, string> = {
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.png': 'image/png'
}

export function registerMediaScheme(): void {
  protocol.registerSchemesAsPrivileged([
    {
      scheme: 'media',
      privileges: { standard: true, secure: true, stream: true, supportFetchAPI: true, corsEnabled: true }
    }
  ])
}

function openFile(path: string, signal: AbortSignal): ReadableStream<Uint8Array> {
  const node = createReadStream(path)
  const abort = (): void => {
    node.destroy()
  }
  if (signal.aborted) node.destroy()
  else {
    signal.addEventListener('abort', abort, { once: true })
    node.once('close', () => signal.removeEventListener('abort', abort))
  }
  node.on('error', (err: NodeJS.ErrnoException) => {
    if (
      signal.aborted ||
      err.name === 'AbortError' ||
      err.code === 'ABORT_ERR' ||
      err.code === 'ERR_STREAM_PREMATURE_CLOSE'
    )
      return
    console.error('[media] read failed', path, err)
  })
  return Readable.toWeb(node) as ReadableStream<Uint8Array>
}

export function handleMediaProtocol(engine: Caller): void {
  const cache = new Map<string, string | null>()
  engine.onEvent((event) => {
    if (event === 'library.reloaded') cache.clear()
  })

  protocol.handle('media', async (request) => {
    const url = new URL(request.url)
    const kind = url.host
    const id = decodeURIComponent(url.pathname.slice(1))
    const sizeParam = url.searchParams.get('size') ?? ''
    const key = `${kind}/${id}/${sizeParam}`
    if (kind !== 'art') return new Response(null, { status: 404 })
    try {
      if (request.signal.aborted) return new Response(null, { status: 499 })

      let path = cache.get(key)
      if (path === undefined) {
        path = await engine.call<string | null>('media.resolve', { kind, id, size: sizeParam })
        cache.set(key, path)
      }
      if (!path) return new Response(null, { status: 404 })

      const info = await stat(path)
      if (request.signal.aborted) return new Response(null, { status: 499 })
      if (!info.isFile()) return new Response(null, { status: 404 })
      const headers: Record<string, string> = {
        'content-type': MIME[extname(path).toLowerCase()] ?? 'application/octet-stream',
        'content-length': String(info.size),
        'access-control-allow-origin': '*'
      }

      if (request.method === 'HEAD') return new Response(null, { status: 200, headers })
      return new Response(openFile(path, request.signal), { status: 200, headers })
    } catch (err) {
      if (request.signal.aborted || (err as Error).name === 'AbortError') return new Response(null, { status: 499 })
      console.error('[media]', key, err)
      return new Response(null, { status: 404 })
    }
  })
}
