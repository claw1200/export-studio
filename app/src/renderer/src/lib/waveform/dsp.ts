// Numeric waveform drawing work. Runs on the main thread for small jobs and in compute.worker.ts for the rest.
// The analysis itself (envelope and band peaks) is done by the native engine.

export type ComputeRequest = { op: 'fall'; column: Uint8Array; left: number; count: number; reach: number }

export type ComputeResponse = { op: 'fall'; out: Uint8Array } | { op: 'error'; message: string }

/** Upper envelope of falls that land `reach` samples ahead. Rises stay flat. */
export function fallSlice(column: Uint8Array, left: number, count: number, reach: number): Uint8Array {
  const out = new Uint8Array(count)
  const end = left + count
  for (let i = left; i < end; i++) {
    let best = column[i] * reach
    const maxK = i < reach ? i : reach - 1
    for (let k = 1; k <= maxK; k++) {
      const peak = column[i - k]
      const dest = column[i - k + reach]
      const scaled = dest >= peak ? peak * reach : peak * (reach - k) + dest * k
      if (scaled > best) best = scaled
    }
    const rounded = Math.round(best / reach)
    out[i - left] = rounded > 255 ? 255 : rounded
  }
  return out
}
