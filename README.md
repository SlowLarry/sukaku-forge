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
