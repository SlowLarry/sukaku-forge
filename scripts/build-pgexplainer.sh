#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
SOURCE_DIR=${1:-"$ROOT/../PGExplainer"}
OUTPUT_JAR=${2:-"$ROOT/target/pgexplainer/PGExplainer.jar"}
PINNED_COMMIT=2f356d6cffbe45e1e7525c2df9ff383b861ada2d
EXPECTED_SHA256=f6e6e3707ba7e774d15125c886a60efffe15717015983757856b186e5a0df525

if [ ! -d "$SOURCE_DIR/.git" ]; then
    echo "error: PGExplainer checkout not found at $SOURCE_DIR" >&2
    echo "clone https://github.com/1to9only/PGExplainer.git there first" >&2
    exit 1
fi

ACTUAL_COMMIT=$(git -C "$SOURCE_DIR" rev-parse HEAD)
if [ "$ACTUAL_COMMIT" != "$PINNED_COMMIT" ]; then
    echo "error: PGExplainer checkout is $ACTUAL_COMMIT, expected $PINNED_COMMIT" >&2
    exit 1
fi

BUILD_DIR=$(mktemp -d "${TMPDIR:-/tmp}/pgexplainer-build.XXXXXX")
trap 'rm -rf "$BUILD_DIR"' EXIT HUP INT TERM
mkdir -p "$BUILD_DIR/classes" "$(dirname -- "$OUTPUT_JAR")"
BUILT_JAR="$BUILD_DIR/PGExplainer.jar"

java -m jdk.compiler/com.sun.tools.javac.Main \
    --release 8 -encoding UTF-8 -d "$BUILD_DIR/classes" \
    "$SOURCE_DIR"/sudoku/*.java
java -m jdk.jartool/sun.tools.jar.Main \
    --create --file "$BUILT_JAR" --date=2022-07-04T08:38:38Z \
    -C "$BUILD_DIR/classes" .

ACTUAL_SHA256=$(python3 - "$BUILT_JAR" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
print(hashlib.sha256(path.read_bytes()).hexdigest())
PY
)
if [ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]; then
    echo "error: non-reproducible PGExplainer JAR: $ACTUAL_SHA256" >&2
    exit 1
fi

cp "$BUILT_JAR" "$OUTPUT_JAR"

echo "built $OUTPUT_JAR"
echo "sha256 $ACTUAL_SHA256"
