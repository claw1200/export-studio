import { createReadStream, type ReadStream } from 'fs'
import { stat } from 'fs/promises'
import { extname } from 'path'
import { Readable } from 'stream'
import { protocol } from 'electron'
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

type ByteRange = { start: number; end: number }

/**
 * `bytes=start-end`, `bytes=start-`, and `bytes=-suffix`.
 * `null` means the header isn't a single range we understand; the caller sends the whole file.
 */
function byteRange(header: string, size: number): ByteRange | 'unsatisfiable' | null {
  const spec = header.split(',')[0]?.trim() ?? ''
  const match = /^bytes=(\d*)-(\d*)$/.exec(spec)
  if (!match) return null
  if (size <= 0) return 'unsatisfiable'

  const startStr = match[1]
  const endStr = match[2]
  if (startStr === '' && endStr === '') return null

  if (startStr === '') {
    const suffix = Number(endStr)
    if (!Number.isFinite(suffix) || suffix <= 0) return 'unsatisfiable'
    return { start: Math.max(0, size - suffix), end: size - 1 }
  }

  const start = Number(startStr)
  if (!Number.isFinite(start) || start < 0 || start >= size) return 'unsatisfiable'
  const end = endStr === '' ? size - 1 : Number(endStr)
  if (!Number.isFinite(end) || end < start) return 'unsatisfiable'
  return { start, end: Math.min(end, size - 1) }
}

function openFile(path: string, signal: AbortSignal, range?: ByteRange): ReadableStream<Uint8Array> {
  const node: ReadStream = range ? createReadStream(path, range) : createReadStream(path)
  const abort = (): void => {
    node.destroy()
  }
  if (signal.aborted) node.destroy()
  else {
    signal.addEventListener('abort', abort, { once: true })
    node.once('close', () => signal.removeEventListener('abort', abort))
  }
  node.on('error', (err: NodeJS.ErrnoException) => {
    if (signal.aborted || err.name === 'AbortError' || err.code === 'ABORT_ERR' || err.code === 'ERR_STREAM_PREMATURE_CLOSE')
      return
    console.error('[media] read failed', path, err)
  })
  return Readable.toWeb(node) as ReadableStream<Uint8Array>
}

/**
 * media://art/<trackId>?size=s|m  and  media://audio/<trackId>
 * Paths are resolved by the engine, so the renderer can only reach files the library references.
 * Ranges are answered here: file:// fetches ignore Range, so <audio> would report nothing
 * seekable and snap currentTime back to 0 on play.
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
    const sizeParam = url.searchParams.get('size') ?? ''
    const key = `${kind}/${id}/${sizeParam}`
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
      const size = info.size
      const type = MIME[extname(path).toLowerCase()] ?? 'application/octet-stream'
      const headers: Record<string, string> = {
        'content-type': type,
        'accept-ranges': 'bytes',
        'content-length': String(size)
      }

      if (request.method === 'HEAD') return new Response(null, { status: 200, headers })

      const requested = request.headers.get('range')
      if (requested) {
        const range = byteRange(requested, size)
        if (range === 'unsatisfiable') {
          return new Response(null, {
            status: 416,
            headers: { ...headers, 'content-range': `bytes */${size}` }
          })
        }
        if (range) {
          headers['content-length'] = String(range.end - range.start + 1)
          headers['content-range'] = `bytes ${range.start}-${range.end}/${size}`
          return new Response(openFile(path, request.signal, range), { status: 206, headers })
        }
      }

      return new Response(openFile(path, request.signal), { status: 200, headers })
    } catch (err) {
      if (request.signal.aborted || (err as Error).name === 'AbortError') return new Response(null, { status: 499 })
      console.error('[media]', key, err)
      return new Response(null, { status: 404 })
    }
  })
}
