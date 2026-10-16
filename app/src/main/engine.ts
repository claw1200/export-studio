import { spawn, type ChildProcessWithoutNullStreams } from 'child_process'
import { createInterface } from 'readline'
import { join, resolve } from 'path'
import { app } from 'electron'
import { is } from '@electron-toolkit/utils'

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void }
type EventListener = (event: string, data: unknown) => void

export class EngineError extends Error {
  constructor(
    message: string,
    public type: string
  ) {
    super(message)
  }
}

function usbExport(): string {
  const name = process.platform === 'win32' ? 'es-usb.exe' : 'es-usb'
  if (is.dev) return resolve(app.getAppPath(), '..', 'vendor', 'usb-sync', 'target', 'release', name)
  return join(process.resourcesPath, 'engine', name)
}

function ffmpegBinary(): string {
  const name = process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg'
  if (is.dev) return resolve(app.getAppPath(), 'resources', 'native', name)
  return join(process.resourcesPath, 'engine', name)
}

function engineCommand(): { cmd: string; args: string[]; cwd: string } {
  if (is.dev) {
    const cwd = resolve(app.getAppPath(), '..', 'engine')
    return { cmd: 'uv', args: ['run', '--quiet', 'exportstudio-engine'], cwd }
  }
  const dir = join(process.resourcesPath, 'engine')
  const exe = process.platform === 'win32' ? 'exportstudio-engine.exe' : 'exportstudio-engine'
  return { cmd: join(dir, exe), args: [], cwd: dir }
}

export class Engine {
  private proc: ChildProcessWithoutNullStreams | null = null
  private nextId = 1
  private pending = new Map<number, Pending>()
  private listeners = new Set<EventListener>()
  private ready: Promise<void> | null = null

  start(): Promise<void> {
    if (this.ready) return this.ready
    const { cmd, args, cwd } = engineCommand()
    this.ready = new Promise((resolveReady, rejectReady) => {
      const proc = spawn(cmd, args, {
        cwd,
        env: {
          ...process.env,
          PYTHONUNBUFFERED: '1',
          EXPORTSTUDIO_USB_EXPORT: usbExport(),
          EXPORTSTUDIO_FFMPEG: ffmpegBinary()
        }
      })
      this.proc = proc

      createInterface({ input: proc.stdout, crlfDelay: Infinity }).on('line', (line) => {
        let msg: {
          id?: number
          result?: unknown
          error?: { message: string; type: string }
          event?: string
          data?: unknown
        }
        try {
          msg = JSON.parse(line)
        } catch {
          console.error('[engine] bad line', line.slice(0, 200))
          return
        }
        if (msg.event) {
          if (msg.event === 'engine.ready') resolveReady()
          for (const l of this.listeners) l(msg.event, msg.data)
          return
        }
        const p = msg.id != null ? this.pending.get(msg.id) : undefined
        if (!p) return
        this.pending.delete(msg.id!)
        if (msg.error) p.reject(new EngineError(msg.error.message, msg.error.type))
        else p.resolve(msg.result)
      })

      proc.stderr.on('data', (d) => process.stderr.write(d))
      proc.on('error', (err) => rejectReady(err))
      proc.on('exit', (code) => {
        for (const p of this.pending.values()) p.reject(new Error(`Engine exited (${code})`))
        this.pending.clear()
        this.proc = null
        this.ready = null
        for (const l of this.listeners) l('engine.exit', { code })
      })
    })
    return this.ready
  }

  async call<T = unknown>(method: string, params?: unknown): Promise<T> {
    await this.start()
    const id = this.nextId++
    return new Promise<T>((res, rej) => {
      this.pending.set(id, { resolve: res as (v: unknown) => void, reject: rej })
      this.proc!.stdin.write(JSON.stringify({ id, method, params: params ?? {} }) + '\n')
    })
  }

  onEvent(listener: EventListener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  stop(): void {
    this.proc?.stdin.end()
    this.proc?.kill()
    this.proc = null
  }
}
