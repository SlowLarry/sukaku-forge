import { describe, expect, it, vi } from 'vitest'
import type { ApplicationPort, ApplicationResponseDto } from './applicationPort'
import { PROTOCOL_VERSION } from './applicationPort'
import { createPlatformPort, isTauriRuntime } from './platformPort'

const response: ApplicationResponseDto = {
  protocol_version: PROTOCOL_VERSION,
  request_id: 1,
  response: 'error',
  error: { code: 'test', message: 'test response' },
}

describe('platform application-port selection', () => {
  it('recognizes only a Tauri-owned runtime marker', () => {
    expect(isTauriRuntime({ __TAURI_INTERNALS__: {} })).toBe(true)
    expect(isTauriRuntime({})).toBe(false)
    expect(isTauriRuntime(null)).toBe(false)
  })

  it('uses the WASM Worker outside a Tauri webview without loading Tauri', async () => {
    const webPort: ApplicationPort = { dispatch: vi.fn(async () => response) }
    const createWasmPort = vi.fn(() => webPort)
    const loadTauriCore = vi.fn()

    const selected = await createPlatformPort({
      runtime: {},
      createWasmPort,
      loadTauriCore,
    })

    expect(selected).toBe(webPort)
    expect(createWasmPort).toHaveBeenCalledOnce()
    expect(loadTauriCore).not.toHaveBeenCalled()
  })

  it('lazily loads Tauri and forwards through the native command adapter', async () => {
    const invoke = vi.fn(async () => JSON.stringify(response))
    const loadTauriCore = vi.fn(async () => ({ invoke }))
    const createWasmPort = vi.fn()
    const port = await createPlatformPort({
      runtime: { __TAURI_INTERNALS__: {} },
      createWasmPort,
      loadTauriCore,
    })

    await expect(port.dispatch({
      protocol_version: PROTOCOL_VERSION,
      request_id: 1,
      command: 'next_hint',
      expected_revision: '0',
    })).resolves.toEqual(response)
    expect(loadTauriCore).toHaveBeenCalledOnce()
    expect(createWasmPort).not.toHaveBeenCalled()
    expect(invoke).toHaveBeenCalledWith('dispatch_json', expect.any(Object))
  })
})
