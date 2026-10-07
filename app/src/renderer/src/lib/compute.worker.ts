import { fallSlice, scanPart, type ComputeRequest, type ComputeResponse, type ScanSpec } from './dsp'

interface WorkerScope {
  onmessage: ((event: MessageEvent<ComputeRequest>) => void) | null
  postMessage(message: ComputeResponse, transfer?: Transferable[]): void
}

const scope = self as unknown as WorkerScope

scope.onmessage = (event: MessageEvent<ComputeRequest>) => {
  const msg = event.data
  try {
    if (msg.op === 'fall') {
      const out = fallSlice(msg.column, msg.left, msg.count, msg.reach)
      scope.postMessage({ op: 'fall', out }, [out.buffer])
      return
    }
    const spec: ScanSpec = msg
    const result = scanPart(spec, msg.samples)
    const transfer: Transferable[] = []
    if (result.bands.byteLength) transfer.push(result.bands.buffer)
    if (result.env.byteLength) transfer.push(result.env.buffer)
    scope.postMessage({ op: 'scan', ...result }, transfer)
  } catch (err) {
    scope.postMessage({ op: 'error', message: err instanceof Error ? err.message : 'compute failed' })
  }
}
