import { readFile } from 'node:fs/promises'

const loaderPath = new URL('../apps/gui/public/wasm/sukaku_forge_wasm_api.js', import.meta.url)
const binaryPath = new URL('../apps/gui/public/wasm/sukaku_forge_wasm_api_bg.wasm', import.meta.url)
const { WasmApplicationPort, initSync } = await import(loaderPath.href)
initSync({ module: await readFile(binaryPath) })

const port = new WasmApplicationPort()
const dispatch = (request) => JSON.parse(port.dispatch_json(JSON.stringify(request)))
const created = dispatch({
  protocol_version: 2,
  request_id: 1,
  command: 'create_session',
  puzzle: '53..7....6..195....98....6.8...6...34..8.3..17...2...6.6....28....419..5....8..79',
})

if (created.protocol_version !== 2 || created.request_id !== 1 || created.response !== 'session_created') {
  throw new Error(`unexpected WASM create response: ${JSON.stringify(created)}`)
}

const hinted = dispatch({
  protocol_version: 2,
  request_id: 2,
  command: 'next_hint',
  expected_revision: created.snapshot.revision,
})
if (hinted.response !== 'next_hint' || hinted.outcome !== 'presented') {
  throw new Error(`unexpected WASM next-hint response: ${JSON.stringify(hinted)}`)
}

const applied = dispatch({
  protocol_version: 2,
  request_id: 3,
  command: 'apply_hint',
  expected_revision: hinted.revision,
  hint_id: hinted.hint_id,
})
if (applied.response !== 'snapshot' || applied.snapshot.revision !== '1') {
  throw new Error(`unexpected WASM apply response: ${JSON.stringify(applied)}`)
}

console.log('WASM application-port smoke test passed')
