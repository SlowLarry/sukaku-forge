import type { ApplicationPort } from './applicationPort'
import { createTauriPort, type TauriInvoke } from './tauriPort'
import { WasmWorkerApplicationPort } from './wasmApplicationPort'

type TauriCore = { invoke: TauriInvoke }

export interface PlatformPortDependencies {
  runtime?: unknown
  loadTauriCore?: () => Promise<TauriCore>
  createWasmPort?: () => ApplicationPort
}

const isRecord = (value: unknown): value is Record<string, unknown> => (
  value != null && typeof value === 'object' && !Array.isArray(value)
)

/** Tauri v2 exposes this runtime-owned global only inside its webview. */
export const isTauriRuntime = (runtime: unknown = globalThis): boolean => (
  isRecord(runtime) && '__TAURI_INTERNALS__' in runtime
)

const loadTauriCore = async (): Promise<TauriCore> => {
  const { invoke } = await import('@tauri-apps/api/core')
  return { invoke: (command, args) => invoke(command, args) }
}

/** Select the thin platform transport while keeping React transport-agnostic. */
export async function createPlatformPort({
  runtime = globalThis,
  loadTauriCore: loadDesktop = loadTauriCore,
  createWasmPort = () => new WasmWorkerApplicationPort(),
}: PlatformPortDependencies = {}): Promise<ApplicationPort> {
  if (!isTauriRuntime(runtime)) return createWasmPort()
  const desktop = await loadDesktop()
  return createTauriPort(desktop.invoke)
}
