# Sukaku Forge

Sukaku Forge is a behavior-compatible Rust reimplementation of
[Sukaku Explainer](https://github.com/dclamage/SukakuExplainer). It is being
built against a frozen, optimized Java reference and its exact solve-trace
oracle rather than by translating the legacy object model line by line.

The project is currently **0.1.0**. Version 1.0 is reserved for complete rating
and trace compatibility with the Java reference on the committed classic and
variant corpus.

## Workspace

- `sukaku-forge-core`: compact masks, puzzle state and immutable constraint
  topology;
- `sukaku-forge-engine`: ordered logical inference and rating pipeline;
- `sukaku-forge-presentation`: semantic hint views and versioned wire DTOs;
- `sukaku-forge-app`: authoritative revisioned GUI sessions;
- `sukaku-forge-wasm-api`: browser-worker lifetime and JSON-forwarding wrapper;
- `sukaku-forge`: command-line application;
- `apps/gui`: the shared React/SVG desktop and browser interface;
- `apps/gui/src-tauri`: the native Tauri command shell.

The current compatibility layer includes the ordered topology/state kernel and
the complete ordinary direct-producer group: Hidden Single, Direct
Pointing/Claiming, Direct Hidden Pair, Naked Single and Direct Hidden Triplet.
The first indirect layer is also present: ordinary Pointing/Claiming and
Generalized Intersections, including anti-knight common-visibility deductions,
followed by the pair/triplet set and fish tiers: ordinary and generalized Naked
Pairs and Triplets, X-Wing, Swordfish, Hidden Pair and Hidden Triplet. The
original-rating StrongLinks(2) tier and the behaviorally distinct revised
legacy Turbot Fish are ported as well, followed by XY-Wing, XYZ-Wing and all
four Unique Rectangle/Loop types. The complete quad tier follows: ordinary and
generalized Naked Quad, Jellyfish and Hidden Quad, followed by StrongLinks(3).
The standard/revised producer registries preserve Java's different order, its topology-sensitive
ordinary/generalized-set defaults, and its revised direct triplet enable-gate
quirk. Enabled but unported producers stop with an explicit port gap; they are
never skipped to manufacture a plausible but incorrect rating.

Forcing Chains & Cycles now reproduces Java's ordered X-only, Y-only and mixed
static implication searches, including its length-based ratings and Turbot
Fish alias, without recreating Java `Potential` object graphs. Aligned Triplet
Exclusion follows with Java's exact base-triplet, common-excluder and candidate-
combination order and its fixed 7.5 rating. Nishio Forcing Chains now reproduce
Java's dynamic binary-contradiction search, length-based 7.5+ ratings, and exact
source-candidate effects in both compatibility modes. Multiple Forcing Chains
add Java-ordered cell and region branch intersections over the static X/Y
implication graph. Level-0 Dynamic Forcing Chains reuse those intersections
with live candidate rollback, hidden-parent ancestry, binary contradictions and
double reductions. Both families retain Java's raw floating-point ranking and
length-based ratings in Original and Revised mode. Dynamic Forcing Chains (+)
then adds Java's ordered advanced-deduction fixed point. Nested levels 2 and 3
embed the complete static Forcing Chains and Multiple Forcing Chains result
sets, while level 4 embeds capped dynamic chains. The registry preserves the
legacy mode distinction: Original tries level-4 caps 0 through 3, while Revised
tries caps 0 through 2.

## Compatibility and Forge modes

The default `Original` mode is the frozen legacy SE rating system used by the
Sudoku community. `Revised` separately reproduces Java's optional revised
table and ordering. Neither compatibility mode will drift. The orthogonal
`SearchPolicy::Compatibility`/`SearchPolicy::Forge` axis now reserves an
explicit opt-in boundary for future ordering or technique experiments; Forge
initially delegates to the exact selected compatibility registry. The CLI uses
`--search-policy=compatibility|forge` (or `--forge`). See
[`docs/COMPATIBILITY_MODES.md`](docs/COMPATIBILITY_MODES.md) for the frozen
two-dimensional contract and the complete Revised-mode trace oracle.

## Development

```sh
make verify
cargo run -p sukaku-forge -- inspect --anti-knight PUZZLE
cargo run -p sukaku-forge -- trace PUZZLE
cargo build --workspace --release
make build-native
make gui-check
make gui-dev
make build-web
make build-pgexplainer
python3 scripts/benchmark-java-rust.py --runs 3
```

## Windows packages

The `Windows desktop` GitHub Actions workflow builds the Tauri `.exe` and
`.msi` installers on a native Windows runner. Pushes to `main` and manual runs
store the installers as downloadable workflow artifacts. Pushing a tag that
exactly matches the application version, such as `v0.1.0`, additionally creates
a draft GitHub Release with those installers attached.

The packages are currently unsigned development builds. Windows may display a
SmartScreen warning until an Authenticode certificate is configured for the
release workflow.

`make build-native` writes a host-tuned binary to
`target/native/release/sukaku-forge`. It is intended for local rating and
benchmark runs only: `target-cpu=native` can select instructions unavailable
on another machine, so portable release artifacts must continue to use the
ordinary Cargo release build.

The benchmark also includes pinned
[PGExplainer](https://github.com/1to9only/PGExplainer) for supported classic
cases. Build its reproducible JAR with `make build-pgexplainer`, or use
`--without-pg` when only the three trace-capable engines are wanted. PG is
reported as `pg-upstream-parallel`: its shipped chaining implementation is
multithreaded and cannot be forced into the single-thread policy used by the
other rows. It is rating/timing-only and is never included in trace consensus.
See [`docs/PGEXPLAINER.md`](docs/PGEXPLAINER.md).

The protected 11.8 major-milestone case is never selected implicitly. An
authorized run must explicitly select the case, acknowledge the gate, and use
one run and one copy. The harness then performs exactly one timed solve per
engine and requires the pinned original, optimized Java and Rust ratings to
agree. The first authorized single-copy run froze `11.8/1.2/1.2`: 422.367s
for pinned Java, 132.326s for optimized Java and 69.719s for release Rust.

Full-trace parity for that protected case has a separate, equally explicit
gate:

```sh
python3 scripts/verify-protected-trace.py \
  --case user_extreme_major_milestone_probe \
  --allow-major-milestone
```

The verifier checks the pinned original JAR SHA-256 before starting, then runs
original Java, optimized Java and release Rust exactly once each, sequentially.
It keeps compact `STEP`/`RESULT` output in memory only and directly compares
every canonical v1 record, the complete `RESULT`, rating, final state and both
digests. It prints elapsed time and the resulting contract for each engine. No
protected solve is part of `make verify`; that target only runs synthetic gate,
execution-count and trace-comparison tests. The first authorized capture froze
133 exact steps, final grid/state and whole-trace digest
`594d23c1329c5ad8c9a884a093a72d2b7c1d981936137ee050f034d01d4c2ef7`;
later authorized runs must reproduce that contract.

After the chain-cache, compact implication-table and StrongLinks(4) cache work,
the same protected 133-step replay measured 436.813s in pinned Java, 131.408s
in optimized Java and 22.506s in host-native Rust on that machine. These are
fresh-process host measurements, not portable guarantees; the exact ordered
trace remains the stronger contract.

The topology verification serializes all 1,024 Java-compatible topology
configurations and requires the exact Java SHA-256 fingerprint. Trace tests
also replay 184 mid-solve Java snapshots so solved cells and unresolved
singleton candidates cannot be confused.

The two committed hard paths now solve completely with exact Java traces and
RESULT identities: 72 steps at 8.9/1.5/1.5 for the classic dynamic-chain case,
and 114 steps at 8.3/1.2/1.2 for the anti-knight case. Multiple Forcing Chains
produce classic steps 8–9 and anti-knight steps 42–44; level-0 Dynamic Forcing
Chains produce classic steps 10–11. The canonical state replays pin all seven
occurrences independently in both Original and Revised mode. WXYZ-Wing through
TUVWXYZ-Wing, all four Bivalue Universal Grave subtypes, StrongLinks(4),
Aligned Pair Exclusion, Forcing Chains & Cycles, Aligned Triplet Exclusion and
Nishio Forcing Chains remain covered along the completed paths. BUG
retains the default lkSudoku fix and its legacy-off compatibility branch while
using primitive stripped-candidate state instead of cloning a second grid. The
VWXYZ-Wing and UVWXYZ-Wing are covered by selected full-registry Java replays.
TUVWXYZ-Wing is reached in full-trace verification; because neither
committed trace selects it, its positive-result coverage remains in direct
Java-derived fixtures. Unique Loops use an allocation-free
18-cell DFS with incremental region-parity validation while preserving Java's
discovery and raw floating-point sort order.

DFC+ and the enabled nested levels are now ported. Compact trace contracts cover
the normal 9.6/9.8 and 10.4/10.5 benchmark families; focused release-oracle
fixtures additionally pin level 3 at 11.0/complexity 116 and level-4 cap 0 at
11.7/complexity 207. Nested proofs retain Java's ordered `FullChain` identity,
recursive complexity and parent dependencies without retaining complete branch
arenas for every losing hint. Compact removal maps preserve the legacy
default-capacity iteration order because nested contradiction selection can
observe it. The protected community 11.8 benchmark remains double-gated; its
first authorized run produced the frozen `11.8/1.2/1.2` rating in all three
engines.

Java's nondefault `FCPlus=1` and `FCPlus=2` schedules are also represented.
`FCPlus=1` adds XY-/XYZ-Wing implications and matches both Java engines on the
normal 9.6 hard case at `9.7/9.7/9.5` in Original and Revised mode. `FCPlus=2`
adds the well-defined triplet, Swordfish and alphabet-wing families. Released
Java then throws when a productive Aligned Exclusion, Unique Loop or BUG lacks
the legacy parent interface; the checked Rust path reports that exact boundary
as `Incomplete` instead of inventing a different rating. Use `--FCPlus=0|1|2`
or Java's `-P` spelling.

All four early non-consecutive producers are now ported for `--isNC=1..4`:
orthogonal and Ferz forcing-cell NC at 2.4, followed by their locked-NC rules at
2.5. Complete Java-derived direct paths cover 29 orthogonal hints and 19 cyclic
Ferz hints, including cyclic digit order, visibility filtering and the released
toroidal Ferz/Wazir quirk.

## GUI direction

The GUI is React/TypeScript with a layered SVG board and semantic, color-free
hint DTOs. A Rust `Session` owns the grid, retained hints, revision checks and
exact undo/redo; clients replace their full snapshot after each mutation and
can apply only an opaque server-retained hint ID. The browser runtime owns the
dispatcher in a WASM module Worker. The native shell exposes the same
dispatcher through a Tauri command that runs solver work on the blocking pool.
See
[`docs/GUI_ARCHITECTURE.md`](docs/GUI_ARCHITECTURE.md).

## License

Sukaku Forge is derived from Sukaku Explainer and is distributed under the GNU
Lesser General Public License, version 2.1 or later. See `LICENSE`.
