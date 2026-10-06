import { createReadStream } from 'fs'
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

async function serveFile(path: string, range: string | null): Promise<Response> {
  const { size } = await stat(path)
  const type = MIME[extname(path).toLowerCase()] ?? 'application/octet-stream'
  const base = { 'Content-Type': type, 'Accept-Ranges': 'bytes', 'Cache-Control': 'max-age=3600' }

  const match = range && /bytes=(\d*)-(\d*)/.exec(range)
  if (match) {
    const start = match[1] ? Number(match[1]) : Math.max(0, size - Number(match[2]))
    const end = match[1] && match[2] ? Math.min(Number(match[2]), size - 1) : size - 1
    if (start >= size || start > end) {
      return new Response(null, { status: 416, headers: { 'Content-Range': `bytes */${size}` } })
    }
    const body = Readable.toWeb(createReadStream(path, { start, end })) as ReadableStream
    return new Response(body, {
      status: 206,
      headers: { ...base, 'Content-Range': `bytes ${start}-${end}/${size}`, 'Content-Length': String(end - start + 1) }
    })
  }
  const body = Readable.toWeb(createReadStream(path)) as ReadableStream
  return new Response(body, { status: 200, headers: { ...base, 'Content-Length': String(size) } })
}

/**
 * media://art/<trackId>?size=s|m  and  media://audio/<trackId>
 * Paths are resolved by the engine, so the renderer can only reach files the library references.
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
      return await serveFile(path, request.headers.get('range'))
    } catch (err) {
      console.error('[media]', key, err)
      return new Response(null, { status: 500 })
    }
  })
}
