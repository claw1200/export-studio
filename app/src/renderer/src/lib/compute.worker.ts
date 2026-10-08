import { fallSlice, type ComputeRequest, type ComputeResponse } from './dsp'

interface WorkerScope {
  onmessage: ((event: MessageEvent<ComputeRequest>) => void) | null
  postMessage(message: ComputeResponse, transfer?: Transferable[]): void
}

const scope = self as unknown as WorkerScope

scope.onmessage = (event: MessageEvent<ComputeRequest>) => {
  const msg = event.data
  try {
    const out = fallSlice(msg.column, msg.left, msg.count, msg.reach)
    scope.postMessage({ op: 'fall', out }, [out.buffer])
  } catch (err) {
    scope.postMessage({ op: 'error', message: err instanceof Error ? err.message : 'compute failed' })
  }
}
