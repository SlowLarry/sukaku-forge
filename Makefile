.PHONY: build build-native build-pgexplainer check fmt gui-check gui-dev test verify

build:
	cargo build --workspace

# Host-tuned release build for local rating and benchmark runs. Keep it in a
# separate target directory so it cannot replace the portable release binary.
build-native:
	CARGO_TARGET_DIR=target/native RUSTFLAGS="-C target-cpu=native" cargo build --workspace --release

build-pgexplainer:
	sh scripts/build-pgexplainer.sh

fmt:
	cargo fmt --all --check

test:
	cargo test --workspace

check:
	cargo clippy --workspace --all-targets -- -D warnings

gui-check:
	cd apps/gui && npm ci && npm run typecheck && npm run lint && npm test && npm run build

gui-dev:
	cd apps/gui && npm run dev -- --host 127.0.0.1

verify: fmt check test build
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts/test_benchmark_java_rust.py
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts/test_verify_protected_trace.py
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts/test_verify_revised_trace.py
	sh scripts/verify-java-topology.sh
	python3 scripts/verify-hidden-single-oracle.py
	python3 scripts/verify-direct-oracle.py
	python3 scripts/verify-ported-prefix-oracle.py
	python3 scripts/verify-revised-trace.py
