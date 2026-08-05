#!/bin/sh
set -eu

expected=a8d844f95339c76c3a385d0871096cd89290fa557f694a98a4df86eca55b879a
actual=$(cargo run --quiet -p sukaku-forge-core --example topology_digest | sha256sum)
actual=${actual%% *}

if [ "$actual" != "$expected" ]; then
    echo "Java topology fingerprint mismatch: expected $expected, got $actual" >&2
    exit 1
fi

echo "PASS Java topology fingerprint (1024 configurations, SHA-256 $actual)"
