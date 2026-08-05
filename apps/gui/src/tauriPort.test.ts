import { describe, expect, it, vi } from 'vitest'
import golden from './fixtures/protocol-v2-hidden-single.json'
import {
  ApplicationProtocolError,
  type ApplicationRequestDto,
} from './applicationPort'
import { createTauriPort } from './tauriPort'

describe('Tauri application-port adapter', () => {
  it('sends owned JSON to dispatch_json and validates the correlated response', async () => {
    const step = golden.steps[0]!
    const request = step.request as ApplicationRequestDto
    const invoke = vi.fn(async () => JSON.stringify(step.response))
    const port = createTauriPort(invoke)

    const response = await port.dispatch(request)

    expect(invoke).toHaveBeenCalledOnce()
    expect(invoke).toHaveBeenCalledWith('dispatch_json', {
      request: JSON.stringify(request),
    })
    expect(response.response).toBe('session_created')
  })

  it('rejects non-string, malformed, and uncorrelated command responses', async () => {
    const request = golden.steps[0]!.request as ApplicationRequestDto

    await expect(createTauriPort(async () => ({})).dispatch(request))
      .rejects.toThrowError(/must return a JSON string/)
    await expect(createTauriPort(async () => '{').dispatch(request))
      .rejects.toThrowError(/returned malformed JSON/)

    const wrongRequestId = structuredClone(golden.steps[0]!.response)
    wrongRequestId.request_id = 99
    await expect(createTauriPort(async () => JSON.stringify(wrongRequestId)).dispatch(request))
      .rejects.toBeInstanceOf(ApplicationProtocolError)
  })
})
