import {
  ApplicationProtocolError,
  type ApplicationPort,
  type ApplicationRequestDto,
  type ApplicationResponseDto,
  parseApplicationResponse,
} from './applicationPort'

type PendingRequest = {
  resolve: (response: ApplicationResponseDto) => void
  reject: (error: Error) => void
}

type WorkerEndpoint = Pick<Worker, 'postMessage' | 'terminate'> & {
  onmessage: ((event: MessageEvent<unknown>) => void) | null
  onerror: ((event: ErrorEvent) => void) | null
  onmessageerror: ((event: MessageEvent<unknown>) => void) | null
}

const createWorker = (): WorkerEndpoint => new Worker(
  new URL('./wasmWorker.ts', import.meta.url),
  { name: 'sukaku-forge-wasm', type: 'module' },
)

/**
 * Promise-based browser adapter for the Rust application port owned by a
 * dedicated module Worker.
 *
 * Calls may be queued concurrently; the Worker executes them synchronously in
 * arrival order. Closing the adapter terminates the Worker and its Rust
 * session. This boundary does not provide cooperative cancellation.
 */
export class WasmWorkerApplicationPort implements ApplicationPort {
  private readonly worker: WorkerEndpoint
  private readonly pending = new Map<number, PendingRequest>()
  private closed = false

  constructor(worker: WorkerEndpoint = createWorker()) {
    this.worker = worker
    worker.onmessage = this.handleMessage
    worker.onerror = this.handleWorkerError
    worker.onmessageerror = this.handleMessageError
  }

  dispatch(request: ApplicationRequestDto): Promise<ApplicationResponseDto> {
    if (this.closed) {
      return Promise.reject(new Error('the WASM application port is closed'))
    }
    if (this.pending.has(request.request_id)) {
      return Promise.reject(new Error(`request ${request.request_id} is already pending`))
    }

    let json: string
    try {
      json = JSON.stringify(request)
    } catch (error) {
      return Promise.reject(asError(error, 'request could not be serialized'))
    }

    return new Promise((resolve, reject) => {
      this.pending.set(request.request_id, { resolve, reject })
      try {
        this.worker.postMessage(json)
      } catch (error) {
        this.pending.delete(request.request_id)
        reject(asError(error, 'request could not be posted to the WASM Worker'))
      }
    })
  }

  /** Destroy the worker-owned Rust session and reject every queued request. */
  close(): void {
    this.fail(new Error('the WASM application port was closed'))
  }

  private readonly handleMessage = (event: MessageEvent<unknown>): void => {
    if (typeof event.data !== 'string') {
      this.fail(new ApplicationProtocolError('the WASM Worker response must be JSON text'))
      return
    }

    let raw: unknown
    try {
      raw = JSON.parse(event.data)
    } catch {
      this.fail(new ApplicationProtocolError('the WASM Worker returned invalid JSON'))
      return
    }

    const requestId = responseRequestId(raw)
    if (requestId == null) {
      this.fail(new ApplicationProtocolError('the WASM Worker response has no valid request_id'))
      return
    }
    const pending = this.pending.get(requestId)
    if (pending == null) {
      this.fail(new ApplicationProtocolError(`the WASM Worker returned unknown request ${requestId}`))
      return
    }

    try {
      const response = parseApplicationResponse(raw, requestId)
      this.pending.delete(requestId)
      pending.resolve(response)
    } catch (error) {
      this.fail(asError(error, 'the WASM Worker returned an invalid response'))
    }
  }

  private readonly handleWorkerError = (event: ErrorEvent): void => {
    event.preventDefault()
    this.fail(new Error(event.message || 'the WASM Worker failed'))
  }

  private readonly handleMessageError = (): void => {
    this.fail(new Error('the WASM Worker response could not be deserialized'))
  }

  private fail(error: Error): void {
    if (!this.closed) {
      this.closed = true
      this.worker.terminate()
    }
    this.rejectAll(error)
  }

  private rejectAll(error: Error): void {
    for (const pending of this.pending.values()) pending.reject(error)
    this.pending.clear()
  }
}

const responseRequestId = (value: unknown): number | null => {
  if (value == null || typeof value !== 'object' || Array.isArray(value)) return null
  const requestId = (value as Record<string, unknown>).request_id
  return Number.isInteger(requestId) && (requestId as number) >= 0 && (requestId as number) <= 4_294_967_295
    ? requestId as number
    : null
}

const asError = (value: unknown, fallback: string): Error => value instanceof Error
  ? value
  : new Error(fallback)
