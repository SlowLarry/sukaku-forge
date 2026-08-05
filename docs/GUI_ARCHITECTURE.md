# GUI architecture

## Goal

One React/TypeScript interface serves desktop and browser builds while the Rust
engine remains the sole authority for puzzle state, solving, hint application
and history. Platform adapters transport the same versioned DTOs; they do not
implement Sudoku rules.

## Boundaries

- `sukaku-forge-core` owns compact grid state and immutable constraint
  topology.
- `sukaku-forge-engine` finds ordered inferences. Its normal rating path stays
  allocation-conscious and does not retain GUI proof graphs.
- `sukaku-forge-presentation` converts the selected inference and any selected
  proof into semantic ordered views, links and structured explanation tokens.
- `sukaku-forge-app` owns one mutable `Session` and the transport-neutral
  command dispatcher.
- `apps/gui` maps primitive wire DTOs into renderer models and keeps only local
  interaction state.
- the WASM Worker and Tauri command are thin owners of the same Rust
  `ApplicationPort`.

## Presentation contract

Presentation data describes meaning, not paint. Candidate, cell and region
marks carry role bitmasks. Links carry exact endpoints, semantic kind, cause
and direction; grouped endpoints preserve their complete member list as well
as a representative drawing anchor. Each view has a stable key and label.
Explanations are typed blocks and inline tokens so web, native accessibility
and future exporters do not parse solver prose.

The SVG board uses eight ordered layers:

1. board paper and permanent topology washes;
2. hint-region backgrounds;
3. semantic cell marks;
4. grid and topology boundaries;
5. placed values;
6. proof links and arrowheads;
7. candidates and semantic halos;
8. focus, selection and hit targets.

## Session safety

Every successful mutation replaces the frontend's complete 81-cell snapshot.
The session revision increments on apply, edit, undo and redo. Next-hint and
mutation commands carry the revision they observed. A hint is represented to
clients by an opaque decimal-string ID; apply accepts only that retained ID and
the matching revision, then executes the server-owned `Inference`.

Client-side effects are display-only. Stale replies are ignored using both the
request ID and revision. A board edit invalidates the pending hint. The fixture
reducer remains available for isolated renderer tests but is not a production
Sudoku engine.

## Protocol v2

Requests and responses contain a protocol version and an exactly representable
numeric request ID. Commands and responses use explicit snake-case tags.
Revisions and hint IDs are serialized as decimal strings because Rust `u64`
values exceed JavaScript's exact integer range. Cells, digits, candidate masks
and role masks remain numbers.

Creating a session returns both the initial snapshot and a topology catalog.
The catalog contains active region identities, labels and ordered cell lists,
plus variant indicators needed by non-region overlays. Next-hint results are
tagged as presented, unsupported, none or incomplete. Unsupported presentation
is honest and still retains a server-owned hint that can be applied if the UI
chooses to expose that action.

## Platform adapters

The browser build loads Rust/WASM once in a module Worker. Messages contain the
same protocol JSON used by native tests, keeping synchronous solver work off
the browser UI thread. Terminating the Worker would destroy the session, so
Cancel remains disabled until the engine has cooperative cancellation points.

The desktop build stores `ApplicationPort` behind `Arc<Mutex<_>>`. Its Tauri
command delegates blocking solver work away from the async runtime and returns
the same response JSON. Tauri APIs are dynamically imported only by the desktop
adapter so the browser bundle does not include them.

## React state

The application receives an asynchronous `ApplicationPort` dependency. It owns
the latest authoritative snapshot, topology and pending presentation, plus
ephemeral UI state such as selected cell, view, filters and busy/error status.
Only one conflicting command is submitted at a time. Get all hints and Solve
remain disabled until Rust exposes those operations; progress is shown only for
real running work.

## Delivery order

1. Freeze semantic presentation and primitive wire DTOs with Rust and Vitest
   contract fixtures.
2. Connect a dependency-injected React session controller to create, next,
   apply, edit, undo and redo.
3. Add the WASM Worker adapter and a real browser smoke test.
4. Add the Tauri adapter over the same dispatcher.
5. Remove the fixture session from production while retaining renderer stories
   and focused geometry tests.
6. Add all-hints collection, persistence, generation and packaging only after
   the single-hint path is stable.

Normal Rust tests, clippy, formatting, GUI unit tests, type checking, lint and
production build are required at every checkpoint. Protocol changes require an
explicit version bump and matching Rust/TypeScript golden updates.
