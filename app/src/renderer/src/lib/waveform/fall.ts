/**
 * "Falling blocks" for the 3-band waveform: each band's level slopes down over the next few
 * blocks instead of dropping at once. Worked out in the worker pool for the whole track and
 * cached per track, so every deck on screen keeps its own.
 */
import { createSubscriber } from 'svelte/reactivity'
import { Listeners } from '../listeners'
import { logicalCount, stridePeak } from './columns'
import { fallSlice } from './dsp'
import { poolSize, runJob } from './pool'

interface FallKey {
  source: Uint8Array
  stride: number
  blocks: number
}

interface FallReady extends FallKey {
  data: Uint8Array
}

/** A retuned setting on a track already worked out waits this long, so a slider drag runs once. */
const RETUNE_MS = 80

function sameKey(key: FallKey | null, source: Uint8Array, stride: number, blocks: number): boolean {
  return !!key && key.source === source && key.stride === stride && key.blocks === blocks
}

function decimateBands(bands: Uint8Array, stride: number): Uint8Array {
  const src = bands.length / 3
  const n = logicalCount(src, stride)
  if (stride <= 1) return bands
  const out = new Uint8Array(n * 3)
  for (let i = 0; i < n; i++) {
    for (let band = 0; band < 3; band++) out[i * 3 + band] = stridePeak(bands, src, i, stride, band)
  }
  return out
}

function columnOf(src: Uint8Array, band: number, reach: number): Uint8Array {
  const n = src.length / 3
  const column = new Uint8Array(n + reach)
  for (let i = 0; i < n; i++) column[i] = src[i * 3 + band]
  const tail = n > 0 ? column[n - 1] : 0
  column.fill(tail, n)
  return column
}

async function computeFall(src: Uint8Array, blocks: number, live: () => boolean): Promise<Uint8Array | null> {
  const n = src.length / 3
  const reach = Math.max(1, Math.round(blocks))
  if (n < 2) return null
  const out = new Uint8Array(src.length)
  const columns = [columnOf(src, 0, reach), columnOf(src, 1, reach), columnOf(src, 2, reach)]
  const cores = poolSize()
  const chunk = Math.ceil(n / cores)
  const work: Array<{ band: number; start: number; count: number; left: number }> = []
  for (let band = 0; band < 3; band++) {
    for (let start = 0; start < n; start += chunk) {
      work.push({ band, start, count: Math.min(chunk, n - start), left: Math.min(start, reach - 1) })
    }
  }
  let next = 0
  let active = 0
  let failed: Error | null = null
  await new Promise<void>((resolve, reject) => {
    const finish = (): void => {
      if (active > 0) return
      if (failed) reject(failed)
      else resolve()
    }
    const kick = (): void => {
      if (failed) return finish()
      if (!live()) return finish()
      while (active < cores && next < work.length && live()) {
        const item = work[next++]
        const column = columns[item.band]
        const copy = new Uint8Array(column.subarray(item.start - item.left, item.start + item.count + reach))
        active++
        runJob({ op: 'fall', column: copy, left: item.left, count: item.count, reach }, [copy.buffer]).then(
          (response) => {
            active--
            if (!failed && live() && response.op === 'fall') {
              const env = response.out
              for (let i = 0; i < env.length; i++) out[(item.start + i) * 3 + item.band] = env[i]
            }
            if (active === 0 && (next >= work.length || !live() || failed)) finish()
            else kick()
          },
          (err: unknown) => {
            active--
            failed = err instanceof Error ? err : new Error('fall failed')
            finish()
          }
        )
      }
      if (active === 0) finish()
    }
    kick()
  })
  return live() ? out : null
}

function fallSync(src: Uint8Array, blocks: number): Uint8Array {
  const n = src.length / 3
  const reach = Math.max(1, Math.round(blocks))
  const out = new Uint8Array(src.length)
  for (let band = 0; band < 3; band++) {
    const column = columnOf(src, band, reach)
    const slice = fallSlice(column, 0, n, reach)
    for (let i = 0; i < n; i++) out[i * 3 + band] = slice[i]
  }
  return out
}

/** Tracks whose falling blocks are kept: every deck on screen, with room to switch back. */
const SLOTS = 4

const fallComputed = new Listeners<[]>()
const subscribe = createSubscriber((update) => fallComputed.add(update))

/**
 * One track's falling blocks: the latest result, and the job working out a new one. Each track
 * has its own, so two decks drawing different tracks never cancel each other's work.
 */
class FallSlot {
  private ready: FallReady | null = null
  private job: FallKey | null = null
  private want: FallKey | null = null
  private timer: ReturnType<typeof setTimeout> | null = null
  private generation = 0

  constructor(private readonly source: Uint8Array) {}

  /**
   * The fallen bands, or null while they are worked out. A retune keeps returning the previous
   * result until the new one is ready.
   */
  get(stride: number, blocks: number): Uint8Array | null {
    const source = this.source
    if (blocks < 1 || source.length < 3) {
      this.cancel()
      return null
    }
    const { ready, job, want } = this
    if (ready && sameKey(ready, source, stride, blocks)) return ready.data
    if (!sameKey(job, source, stride, blocks) && !sameKey(want, source, stride, blocks)) {
      const retune = !!(ready || job || want)
      this.want = { source, stride, blocks }
      if (retune) {
        if (!this.timer) this.timer = setTimeout(() => this.launch(), RETUNE_MS)
      } else {
        this.clearTimer()
        this.launch()
      }
    }
    if (ready && ready.stride === stride) return ready.data
    return null
  }

  /** Drops any queued or running job. A result already worked out is kept. */
  cancel(): void {
    this.want = null
    this.clearTimer()
    this.generation++
    this.job = null
  }

  private clearTimer(): void {
    if (this.timer) clearTimeout(this.timer)
    this.timer = null
  }

  private launch(): void {
    this.timer = null
    const job = this.want
    this.want = null
    if (!job) return
    if (sameKey(this.ready, job.source, job.stride, job.blocks)) return
    const token = ++this.generation
    this.job = job
    void (async () => {
      const decimated = decimateBands(job.source, job.stride)
      let data: Uint8Array | null = null
      try {
        data = await computeFall(decimated, job.blocks, () => token === this.generation)
      } catch (err) {
        if (token !== this.generation) return
        console.warn('[waveform] fall workers failed', err)
        data = fallSync(decimated, job.blocks)
      }
      if (!data || token !== this.generation) return
      this.ready = { ...job, data }
      this.job = null
      fallComputed.notify()
    })()
  }
}

/** Most recently drawn last. */
const slots = new Map<Uint8Array, FallSlot>()

function slotFor(source: Uint8Array): FallSlot {
  const found = slots.get(source)
  if (found) {
    slots.delete(source)
    slots.set(source, found)
    return found
  }
  const slot = new FallSlot(source)
  slots.set(source, slot)
  for (const [oldest, stale] of slots) {
    if (slots.size <= SLOTS) break
    stale.cancel()
    slots.delete(oldest)
  }
  return slot
}

/** Call inside the effect that draws, so it redraws when falling blocks finish. */
export function trackBandFall(): void {
  subscribe()
}

export function bandFall(source: Uint8Array, stride: number, blocks: number): Uint8Array | null {
  return slotFor(source).get(stride, blocks)
}
