# Sukaku Forge

Sukaku Forge is a behavior-compatible Rust reimplementation of the original
Sukaku Explainer and
[SudokuMonster's updated v1.18.1 release](https://github.com/SudokuMonster/SukakuExplainer),
with one React interface shared by the Windows/Tauri and browser/WASM builds.

The project is pre-1.0. Original and Revised compatibility modes preserve the
legacy rating order while the Forge policy provides an explicit seam for
future techniques and optimizations.

## Downloads

Web UI deployment target: [slowlarry.github.io/sukaku-forge](https://slowlarry.github.io/sukaku-forge/).

Windows `.exe` and `.msi` packages are built by the
[Windows desktop workflow](https://github.com/SlowLarry/sukaku-forge/actions/workflows/windows-desktop.yml).
GUI version tags (`v<version>`) publish combined GitHub Releases after the
tagged desktop build and headless-rater validation succeed. Rater-only tags
(`rater-v<version>`) publish only the portable command-line archives when a
milestone has no GUI functionality changes. Current development packages are
unsigned and may trigger a Windows SmartScreen warning.

Relevant versioned [GitHub Releases](https://github.com/SlowLarry/sukaku-forge/releases)
carry portable fast-rater archives for Windows x64 MSVC and static Linux x64
musl, plus a SHA-256 checksum file.

## Fast Classic rater

`sukaku-forge-rate` is a corrected SE 1.2.1-derived, GUI-less rater optimized
for ordinary 9×9 Sudoku. It accepts one 81-character grid as an argument or
one grid per nonempty standard-input line; `.` and `0` mean empty. The default
output is `ER/EP/ED`.

```sh
make build-rater
target/rater/sukaku-forge-rate PUZZLE81

make build-rater-native
target/rater-native/rater/sukaku-forge-rate < puzzles.txt
```

`build-rater-native` uses `target-cpu=native`; its host-specific output is for
local use and is not distributed in releases.

Unique Loops and BUG are disabled by default because they assume a unique
solution. Add `--allow-uniqueness` only for puzzles known to have one.

The focused path removes GUI and presentation work, freezes the Classic
producer schedule, reuses topology and chain-search storage, and rejects
provably impossible inner forcing-chain roots before the original ordered
search. On one pinned Xeon 8370C core, the native 0.5.0 binary completed the
protected 11.8 benchmark in 33.57 seconds; the exact released
[SudokuMonster v1.18.1](https://github.com/SudokuMonster/SukakuExplainer/releases/tag/v1.18.1)
JAR completed it in 761.22 seconds. Both emitted `11.8/1.2/1.2`, a measured
`22.7×` speedup for this case. This is one fresh-process run per engine without
warm-up, not a corpus average; both were pinned to one CPU, Java was forced to
one thread, and Rust used its corrected, uniqueness-off default.

That `22.7×` result is the retained 0.5.0-versus-Java comparison. Separately,
the optimized 0.6.0 code candidate was measured against the preserved native
0.5.0 Rust binary. The isolated and protected benchmark artifact was built
immediately before the metadata bump and therefore still identified itself as
0.5.0. Materializing only the winning chain result removed 912 allocations on
the 10.5 case (runtime neutral); a shared exact-state negative forcing-chain
cache and scratch storage improved its focused run from 3.559 to 2.644 seconds
(`25.7%`, with about 0.57 MiB of negative-cache key payload at the observed
capacity);
delta-aware nested scans added `3.4%`; and a first fixed 9×9 Classic cache
phase added about `0.3%` on 9.8 and `1.6%` on 10.5. End to end, the final 0.6.0
native build reduced the hardest-ten same-core batch from 121.453 to 102.688
seconds (`15.4%`) with identical ratings; the pre-bump protected 11.8 one-shot
fell from 32.891 to 28.949 seconds (`12.0%`) and still emitted `11.8/1.2/1.2`.

See the [Classic-rater documentation](docs/CLASSIC_RATER.md) for the precise
correctness policy, optimized build profiles, batch formatting, oracle pin,
and guarded benchmark harness.

## Build and run

```sh
cargo run -p sukaku-forge -- trace PUZZLE
cargo test --workspace
make gui-dev
make build-web
```

## Documentation

- [Compatibility modes](docs/COMPATIBILITY_MODES.md)
- [Engine architecture](docs/ARCHITECTURE.md)
- [GUI architecture](docs/GUI_ARCHITECTURE.md)
- [PGExplainer benchmark](docs/PGEXPLAINER.md)

## License

Sukaku Forge is distributed under the GNU Lesser General Public License,
version 2.1 or later. See [LICENSE](LICENSE).
