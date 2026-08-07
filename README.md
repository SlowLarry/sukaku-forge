# Sukaku Forge

Sukaku Forge is a behavior-compatible Rust reimplementation of
[Sukaku Explainer](https://github.com/dclamage/SukakuExplainer), with one
React interface shared by the Windows/Tauri and browser/WASM builds.

The project is pre-1.0. Original and Revised compatibility modes preserve the
legacy rating order while the Forge policy provides an explicit seam for
future techniques and optimizations.

## Downloads

Web UI deployment target: [slowlarry.github.io/sukaku-forge](https://slowlarry.github.io/sukaku-forge/).

Windows `.exe` and `.msi` packages are built by the
[Windows desktop workflow](https://github.com/SlowLarry/sukaku-forge/actions/workflows/windows-desktop.yml).
Version tags create draft GitHub Releases. Current development packages are
unsigned and may trigger a Windows SmartScreen warning.

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

Unique Loops and BUG are disabled by default because they assume a unique
solution. Add `--allow-uniqueness` only for puzzles known to have one.

The focused path removes GUI and presentation work, freezes the Classic
producer schedule, reuses topology and chain-search storage, and rejects
provably impossible inner forcing-chain roots before the original ordered
search. On one pinned Xeon 8370C core, the native 0.5.0 binary completed the
protected 11.8 benchmark in 33.1 seconds with `11.8/1.2/1.2`; the reproducibly
pinned SE 1.2.1 Java oracle exceeded a 30-minute timeout. This is a `>54×`
timeout-floor speedup, not a completed Java/Rust ratio or a corpus average:
each engine received one fresh-process run without warm-up, and Rust used its
corrected, uniqueness-off default.

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
