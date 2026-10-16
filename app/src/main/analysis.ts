import { BrowserWindow, ipcMain } from 'electron'
import type { AnalysisFinished, AnalysisJob, AnalysisProgress } from '../preload/analysis'
import type { NativeModule } from './audio/native'
import type { Engine } from './engine'

const MAX_CONCURRENCY = 4

/**
 * Analyses tracks a few at a time: the native analyser works out the grid, key and waveform, and
 * the engine writes them into the library. Progress goes to every window on `analysis:progress`.
 */
export class AnalysisQueue {
  private queue: AnalysisJob[] = []
  private active = new Set<string>()
  private total = 0
  private done = 0
  private failed = 0
  private concurrency = 2

  constructor(
    private library: Engine,
    private native: () => NativeModule | null
  ) {}

  register(): void {
    ipcMain.handle('analysis:enqueue', (_e, jobs: AnalysisJob[], concurrency: number) =>
      this.enqueue(jobs, concurrency)
    )
    ipcMain.handle('analysis:cancel', () => this.cancel())
  }

  private enqueue(jobs: AnalysisJob[], concurrency: number): void {
    this.concurrency = Math.max(1, Math.min(MAX_CONCURRENCY, Math.round(concurrency) || 1))
    const queued = new Set(this.queue.map((job) => job.id))
    const fresh = jobs.filter((job) => !queued.has(job.id) && !this.active.has(job.id))
    if (!fresh.length) return
    this.queue.push(...fresh)
    this.total += fresh.length
    this.broadcast()
    this.pump()
  }

  /** Drops the tracks still waiting. Those already being analysed finish and are saved. */
  private cancel(): void {
    this.total -= this.queue.length
    this.queue = []
    this.broadcast()
    this.settle()
  }

  private pump(): void {
    while (this.active.size < this.concurrency && this.queue.length) {
      const job = this.queue.shift()!
      this.active.add(job.id)
      void this.run(job)
    }
    this.broadcast()
  }

  private async run(job: AnalysisJob): Promise<void> {
    let finished: AnalysisFinished
    try {
      finished = { id: job.id, track: await this.analyse(job) }
      this.done++
    } catch (err) {
      finished = { id: job.id, error: (err as Error).message }
      this.failed++
    }
    this.active.delete(job.id)
    this.broadcast(finished)
    this.pump()
    this.settle()
  }

  private async analyse({ id, options }: AnalysisJob): Promise<unknown> {
    const native = this.native()
    if (!native) throw new Error('The analyser could not be loaded')
    const path = await this.library.call<string | null>('media.resolve', { kind: 'audio', id, size: '' })
    if (!path) throw new Error('Audio file not found')
    const result = await native.analyseTrack(path, {
      grid: options.grid,
      key: options.key,
      dynamic: options.gridMode === 'dynamic',
      minBpm: options.minBpm,
      maxBpm: options.maxBpm,
      highPrecision: options.highPrecision,
      roundBpm: options.roundBpm
    })
    const beats = Array.from(result.beatTimes, (ms, i) => [result.beatNumbers[i], result.beatTempos[i], ms])
    const waveform = Buffer.from(result.waveform.buffer, result.waveform.byteOffset, result.waveform.byteLength)
    return this.library.call('track.applyAnalysis', {
      id,
      result: {
        duration: result.duration,
        bpm: result.bpm,
        beats,
        key: result.key ?? null,
        waveform: waveform.toString('base64')
      },
      options: { grid: options.grid, key: options.key, overwrite: options.overwrite }
    })
  }

  private settle(): void {
    if (this.active.size || this.queue.length) return
    this.total = this.done = this.failed = 0
  }

  private broadcast(finished?: AnalysisFinished): void {
    const progress: AnalysisProgress = {
      total: this.total,
      done: this.done,
      failed: this.failed,
      active: [...this.active],
      ...(finished ? { finished } : {})
    }
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.send('analysis:progress', progress)
    }
  }
}
