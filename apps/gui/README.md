# Sukaku Forge GUI

This is the single React/TypeScript GUI shared by future desktop and browser
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

The renderer vertical slice still uses a typed Classic Sudoku fixture so board,
candidate, region, link and multi-view behavior can be tested independently.
The production boundary is now a validated protocol-v2 `ApplicationPort` model;
the next slice replaces fixture session mutations with authoritative Rust
snapshots through the browser and desktop adapters.
Candidate masks use the same wire bits as Rust and Java: digit `d` is `1 << d`
and the full mask is `0x03fe`.

Planned adapters implement one application port:

- Tauri invokes the native Rust engine on a background task and packages the
  same `dist` output for Windows first, then macOS/Linux.
- The browser adapter sends the same versioned requests to a Web Worker that
  owns the Rust WebAssembly engine.

Both adapters carry a session revision and reject stale hint responses. The mock
state reducer is not a Sudoku constraint engine; checked edits, topology rebuilds
and exact session undo already belong to the Rust app/session layer and must not
be reimplemented in React.

See [DESIGN.md](DESIGN.md) for the design tokens and
[../../docs/GUI_ARCHITECTURE.md](../../docs/GUI_ARCHITECTURE.md) for the full
platform and compatibility plan.
