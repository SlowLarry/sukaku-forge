# Sukaku Forge GUI

This is the single React/TypeScript GUI shared by the desktop and browser
packages. The board is layered SVG; hint and topology data are semantic and do
not contain paint colors.

```sh
npm ci
npm run dev -- --host 127.0.0.1
npm run typecheck
npm run lint
npm test
npm run build
```

The browser transport uses a module Worker that owns one Rust application
port. Generate and smoke-test its web bindings before serving the live app:

```sh
cargo install wasm-bindgen-cli --version 0.2.126 --locked
make build-wasm
make check-wasm
```

`build-wasm` writes generated JavaScript and WebAssembly into the ignored
`public/wasm` asset directory. Solver calls remain synchronous inside the
Worker; terminating it destroys the session and is not cooperative
cancellation.

Create a complete browser distribution, including those generated assets, with
`npm run build:web`. The ordinary `npm run build` remains the frontend-only
build used by the native shell.

For the native shell, install the platform prerequisites documented by Tauri,
then run from this directory:

```sh
npm run tauri -- dev
npm run tauri -- build
```

Production session state comes only from authoritative Rust snapshots through
the validated protocol-v2 `ApplicationPort`. The typed Classic fixture remains
only for test-only renderer, component and session-view coverage.
Candidate masks use the same wire bits as Rust and Java: digit `d` is `1 << d`
and the full mask is `0x03fe`.

Both adapters implement that same application port:

- Tauri invokes the native Rust engine on a blocking-pool task and is configured
  to bundle the same `dist` output. Windows-first distribution, followed by
  macOS/Linux, remains packaging work.
- The browser adapter sends the same versioned requests to a module Worker that
  owns the Rust WebAssembly engine.

React includes the authoritative revision in next-hint and mutation requests;
Rust rejects stale revisions, while the controller correlates responses by
request ID. The adapters only transport protocol JSON. The mock state reducer
is not a Sudoku constraint engine; checked edits, topology rebuilds and exact
session undo belong to the Rust app/session layer.

The current shell boots a built-in Classic puzzle and supports next/apply,
value placement, candidate toggling, undo and redo. Value clearing,
new/open/save, all-hints/Solve, generation, cooperative cancellation and
distributable packaging remain follow-up work.

See [DESIGN.md](DESIGN.md) for the design tokens and
[../../docs/GUI_ARCHITECTURE.md](../../docs/GUI_ARCHITECTURE.md) for the full
platform and compatibility plan.
