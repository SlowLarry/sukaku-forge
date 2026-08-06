import { readdir, readFile, stat } from 'node:fs/promises'

const pagesBase = '/sukaku-forge/'
const distDir = new URL('../dist/', import.meta.url)
const assetsDir = new URL('assets/', distDir)
const wasmDir = new URL('wasm/', distDir)

const assert = (condition, message) => {
  if (!condition) throw new Error(message)
}

const assertNonEmpty = async (path) => {
  const metadata = await stat(path)
  assert(metadata.isFile() && metadata.size > 0, `${path} is missing or empty`)
}

const indexHtml = await readFile(new URL('index.html', distDir), 'utf8')
assert(
  indexHtml.includes(`src="${pagesBase}assets/`),
  `index.html does not load JavaScript from ${pagesBase}`,
)
assert(
  indexHtml.includes(`href="${pagesBase}assets/`),
  `index.html does not load styles from ${pagesBase}`,
)
assert(!indexHtml.includes('src="/assets/'), 'index.html contains a site-root JavaScript URL')
assert(!indexHtml.includes('href="/assets/'), 'index.html contains a site-root stylesheet URL')

const assetNames = await readdir(assetsDir)
const workerName = assetNames.find((name) => /^wasmWorker-.+\.js$/.test(name))
assert(workerName != null, 'the built module Worker is missing')

const workerSource = await readFile(new URL(workerName, assetsDir), 'utf8')
assert(
  workerSource.includes('../wasm/sukaku_forge_wasm_api.js'),
  'the Worker does not resolve the WASM bindings relative to its project-site URL',
)

const entrySources = await Promise.all(
  assetNames
    .filter((name) => name.endsWith('.js') && name !== workerName)
    .map((name) => readFile(new URL(name, assetsDir), 'utf8')),
)
assert(
  entrySources.some((source) => source.includes(`${pagesBase}assets/${workerName}`)),
  'the application entry does not load the Worker from the GitHub Pages base',
)

await Promise.all([
  'sukaku_forge_wasm_api.js',
  'sukaku_forge_wasm_api_bg.wasm',
].map((name) => assertNonEmpty(new URL(name, wasmDir))))

console.log(`verified GitHub Pages bundle at ${pagesBase}`)
