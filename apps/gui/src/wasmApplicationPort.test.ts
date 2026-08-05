import { describe, expect, it } from 'vitest'
import {
  PROTOCOL_VERSION,
  type ApplicationRequestDto,
  type ApplicationResponseDto,
} from './applicationPort'
import { WasmWorkerApplicationPort } from './wasmApplicationPort'

class FakeWorker {
  onmessage: ((event: MessageEvent<unknown>) => void) | null = null
  onerror: ((event: ErrorEvent) => void) | null = null
  onmessageerror: ((event: MessageEvent<unknown>) => void) | null = null
  readonly sent: string[] = []
  terminated = false

  postMessage(message: string): void {
    this.sent.push(message)
  }

  terminate(): void {
    this.terminated = true
  }

  respond(response: unknown): void {
    this.onmessage?.({ data: JSON.stringify(response) } as MessageEvent<string>)
  }

  fail(message: string): void {
    this.onerror?.({
      message,
      preventDefault() {},
    } as ErrorEvent)
  }
}

const request = (requestId: number): ApplicationRequestDto => ({
  protocol_version: PROTOCOL_VERSION,
  request_id: requestId,
  command: 'next_hint',
  expected_revision: '0',
})

const response = (requestId: number): ApplicationResponseDto => ({
  protocol_version: PROTOCOL_VERSION,
  request_id: requestId,
  response: 'error',
  error: { code: 'test_response', message: `response ${requestId}` },
})

describe('WasmWorkerApplicationPort', () => {
  it('sends exact protocol JSON and validates the correlated response', async () => {
    const worker = new FakeWorker()
    const port = new WasmWorkerApplicationPort(worker)
    const pending = port.dispatch(request(7))

    expect(worker.sent).toEqual([JSON.stringify(request(7))])
    worker.respond(response(7))

    await expect(pending).resolves.toEqual(response(7))
  })

  it('correlates concurrent requests even when responses arrive out of order', async () => {
    const worker = new FakeWorker()
    const port = new WasmWorkerApplicationPort(worker)
    const first = port.dispatch(request(1))
    const second = port.dispatch(request(2))

    worker.respond(response(2))
    worker.respond(response(1))

    await expect(first).resolves.toEqual(response(1))
    await expect(second).resolves.toEqual(response(2))
  })

  it('rejects duplicate pending IDs without posting a second command', async () => {
    const worker = new FakeWorker()
    const port = new WasmWorkerApplicationPort(worker)
    const first = port.dispatch(request(4))

    await expect(port.dispatch(request(4))).rejects.toThrow(/already pending/)
    expect(worker.sent).toHaveLength(1)
    worker.respond(response(4))
    await expect(first).resolves.toEqual(response(4))
  })

  it('terminates the worker and rejects pending and future commands on close', async () => {
    const worker = new FakeWorker()
    const port = new WasmWorkerApplicationPort(worker)
    const pending = port.dispatch(request(3))

    port.close()

    expect(worker.terminated).toBe(true)
    await expect(pending).rejects.toThrow(/closed/)
    await expect(port.dispatch(request(5))).rejects.toThrow(/closed/)
  })

  it('closes after a fatal worker error so later commands cannot hang', async () => {
    const worker = new FakeWorker()
    const port = new WasmWorkerApplicationPort(worker)
    const pending = port.dispatch(request(6))

    worker.fail('module load failed')

    expect(worker.terminated).toBe(true)
    await expect(pending).rejects.toThrow(/module load failed/)
    await expect(port.dispatch(request(7))).rejects.toThrow(/closed/)
  })
})
