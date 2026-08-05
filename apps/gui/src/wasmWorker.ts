import { PROTOCOL_VERSION } from './applicationPort'

type RustApplicationPort = {
  dispatch_json(request: string): string
}

type WasmBindings = {
  default(): Promise<unknown>
  WasmApplicationPort: new () => RustApplicationPort
}

type ModuleWorkerScope = {
  readonly location: Location
  addEventListener(type: 'message', listener: (event: MessageEvent<unknown>) => void): void
  postMessage(message: string): void
}

const workerScope = self as unknown as ModuleWorkerScope
let portPromise: Promise<RustApplicationPort> | undefined
let dispatchQueue = Promise.resolve()

const loadPort = async (): Promise<RustApplicationPort> => {
  // `make build-wasm` writes wasm-bindgen's web target beside this URL in the
  // Vite public tree. The ignored import is intentionally resolved at runtime.
  const bindingsUrl = new URL('../wasm/sukaku_forge_wasm_api.js', workerScope.location.href)
  const bindings = await import(/* @vite-ignore */ bindingsUrl.href) as unknown as WasmBindings
  await bindings.default()
  return new bindings.WasmApplicationPort()
}

const getPort = (): Promise<RustApplicationPort> => {
  portPromise ??= loadPort()
  return portPromise
}

const requestIdFrom = (request: unknown): number => {
  if (typeof request !== 'string') return 0
  try {
    const parsed = JSON.parse(request) as unknown
    if (parsed == null || typeof parsed !== 'object' || Array.isArray(parsed)) return 0
    const requestId = (parsed as Record<string, unknown>).request_id
    return Number.isInteger(requestId) && (requestId as number) >= 0 && (requestId as number) <= 4_294_967_295
      ? requestId as number
      : 0
  } catch {
    return 0
  }
}

const transportFailure = (requestId: number, error: unknown): string => JSON.stringify({
  protocol_version: PROTOCOL_VERSION,
  request_id: requestId,
  response: 'error',
  error: {
    code: 'wasm_worker_unavailable',
    message: error instanceof Error ? error.message : 'the Rust WebAssembly module could not be loaded',
  },
})

const dispatch = async (request: unknown): Promise<void> => {
  const requestId = requestIdFrom(request)
  try {
    if (typeof request !== 'string') throw new Error('the WASM Worker request must be JSON text')
    const port = await getPort()
    // Solver execution is deliberately synchronous here, off the UI thread.
    workerScope.postMessage(port.dispatch_json(request))
  } catch (error) {
    workerScope.postMessage(transportFailure(requestId, error))
  }
}

workerScope.addEventListener('message', (event) => {
  dispatchQueue = dispatchQueue.then(() => dispatch(event.data))
})
