import type { ComputeRequest, ComputeResponse } from './dsp'

interface Waiter {
  message: ComputeRequest
  transfer: Transferable[]
  resolve: (value: ComputeResponse) => void
  reject: (error: Error) => void
}

const waiters: Waiter[] = []
const idle: Worker[] = []
let live = 0

export function poolSize(): number {
  const cores = typeof navigator === 'undefined' ? 4 : navigator.hardwareConcurrency || 4
  return Math.max(1, Math.min(8, cores - 2))
}

function pump(): void {
  while (waiters.length && (idle.length > 0 || live < poolSize())) {
    const worker = idle.pop() ?? spawn()
    dispatch(worker, waiters.shift()!)
  }
}

function spawn(): Worker {
  live++
  return new Worker(new URL('./compute.worker.ts', import.meta.url), { type: 'module' })
}

function dispatch(worker: Worker, job: Waiter): void {
  const onMessage = (event: MessageEvent<ComputeResponse>): void => {
    cleanup()
    idle.push(worker)
    if (event.data.op === 'error') job.reject(new Error(event.data.message))
    else job.resolve(event.data)
    pump()
  }
  const onError = (event: ErrorEvent): void => {
    event.preventDefault()
    cleanup()
    live--
    worker.terminate()
    job.reject(new Error(event.message || 'worker failed'))
    pump()
  }
  const cleanup = (): void => {
    worker.removeEventListener('message', onMessage)
    worker.removeEventListener('error', onError)
  }
  worker.addEventListener('message', onMessage)
  worker.addEventListener('error', onError)
  try {
    worker.postMessage(job.message, job.transfer)
  } catch (err) {
    cleanup()
    idle.push(worker)
    job.reject(err instanceof Error ? err : new Error('post failed'))
    pump()
  }
}

export function runJob(message: ComputeRequest, transfer: Transferable[] = []): Promise<ComputeResponse> {
  return new Promise((resolve, reject) => {
    waiters.push({ message, transfer, resolve, reject })
    pump()
  })
}
