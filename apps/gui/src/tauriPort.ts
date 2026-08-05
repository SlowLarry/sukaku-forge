import {
  ApplicationProtocolError,
  dispatchValidated,
  type ApplicationPort,
  type ApplicationRequestDto,
} from './applicationPort'

export type TauriInvoke = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>

const parseCommandResponse = (value: unknown): unknown => {
  if (typeof value !== 'string') {
    throw new ApplicationProtocolError('dispatch_json must return a JSON string')
  }
  try {
    return JSON.parse(value) as unknown
  } catch {
    throw new ApplicationProtocolError('dispatch_json returned malformed JSON')
  }
}

export function createTauriPort(invoke: TauriInvoke): ApplicationPort {
  return {
    dispatch: (request: ApplicationRequestDto) => dispatchValidated(async (command) => {
      const response = await invoke('dispatch_json', { request: JSON.stringify(command) })
      return parseCommandResponse(response)
    }, request),
  }
}
