#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
SOURCE_DIR=${1:-}
OUTPUT_JAR=${2:-"$ROOT/target/se121-oracle/serate.jar"}

PINNED_COMMIT=a4cdac080393a5a17147ab5794a35ed98a5ef2d2
EXPECTED_TREE=e48ef86e71237faedc37f80639348b3933dc60b5
EXPECTED_ARCHIVE_SHA256=0e450e7cc16f3479558eed673c2b657b6153ed6c91eeecbbf4828bdece4dc7b0
EXPECTED_JAR_SHA256=6bc0cebb8bf89563d97ee1f7f0525c4fd021cdf0f19a0dc05b55d769f2bb4797
EXPECTED_JAR_SIZE=163427
EXPECTED_CLASS_COUNT=90
EXPECTED_JAVA_VERSION=17.0.19
EXPECTED_JAVA_VERSION_OUTPUT_SHA256=5cb21f48e9122d5890258861880a6298b1c8e0ab6aa455ebd556989e0e15bd7d
JAR_TIMESTAMP=2019-08-09T00:00:00Z
JAVA_BIN=${SE121_JAVA:-java}

if [ -z "$SOURCE_DIR" ] || [ "$#" -gt 2 ]; then
    echo "usage: $0 GIT_CHECKOUT [OUTPUT_JAR]" >&2
    exit 2
fi

if ! git -C "$SOURCE_DIR" rev-parse --git-dir >/dev/null 2>&1; then
    echo "error: not a Git checkout: $SOURCE_DIR" >&2
    exit 1
fi

if [ "$(git -C "$SOURCE_DIR" cat-file -t "$PINNED_COMMIT" 2>/dev/null || true)" != "commit" ]; then
    echo "error: checkout does not contain pinned commit $PINNED_COMMIT" >&2
    exit 1
fi

ACTUAL_TREE=$(git -C "$SOURCE_DIR" rev-parse "${PINNED_COMMIT}^{tree}")
if [ "$ACTUAL_TREE" != "$EXPECTED_TREE" ]; then
    echo "error: pinned commit tree is $ACTUAL_TREE, expected $EXPECTED_TREE" >&2
    exit 1
fi

JAVA_VERSION=$("$JAVA_BIN" -version 2>&1 | awk -F '"' 'NR == 1 { print $2 }')
if [ "$JAVA_VERSION" != "$EXPECTED_JAVA_VERSION" ]; then
    echo "error: reproducible build requires JDK $EXPECTED_JAVA_VERSION, found ${JAVA_VERSION:-unknown}" >&2
    exit 1
fi
JAVA_VERSION_OUTPUT_SHA256=$("$JAVA_BIN" -version 2>&1 | sha256sum | awk '{ print $1 }')
if [ "$JAVA_VERSION_OUTPUT_SHA256" != "$EXPECTED_JAVA_VERSION_OUTPUT_SHA256" ]; then
    echo "error: reproducible build requires the pinned JDK runtime build" >&2
    echo "error: java -version SHA-256 is $JAVA_VERSION_OUTPUT_SHA256, expected $EXPECTED_JAVA_VERSION_OUTPUT_SHA256" >&2
    exit 1
fi

BUILD_DIR=$(mktemp -d "${TMPDIR:-/tmp}/se121-oracle-build.XXXXXX")
trap 'rm -rf "$BUILD_DIR"' EXIT HUP INT TERM
mkdir -p "$BUILD_DIR/source" "$BUILD_DIR/classes" "$(dirname -- "$OUTPUT_JAR")"
ARCHIVE="$BUILD_DIR/source.tar"
BUILT_JAR="$BUILD_DIR/serate.jar"

git -C "$SOURCE_DIR" archive --format=tar "$PINNED_COMMIT" -o "$ARCHIVE"
ACTUAL_ARCHIVE_SHA256=$(python3 - "$ARCHIVE" <<'PY'
import hashlib
import pathlib
import sys

print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
)
if [ "$ACTUAL_ARCHIVE_SHA256" != "$EXPECTED_ARCHIVE_SHA256" ]; then
    echo "error: pinned source archive is $ACTUAL_ARCHIVE_SHA256, expected $EXPECTED_ARCHIVE_SHA256" >&2
    exit 1
fi
tar -xf "$ARCHIVE" -C "$BUILD_DIR/source"

"$JAVA_BIN" -m jdk.compiler/com.sun.tools.javac.Main \
    --release 8 -encoding windows-1252 \
    -sourcepath "$BUILD_DIR/source/Sudoku" \
    -d "$BUILD_DIR/classes" \
    "$BUILD_DIR/source/Sudoku/diuf/sudoku/test/serate.java"

ACTUAL_CLASS_COUNT=$(find "$BUILD_DIR/classes" -type f -name '*.class' | wc -l | tr -d ' ')
if [ "$ACTUAL_CLASS_COUNT" != "$EXPECTED_CLASS_COUNT" ]; then
    echo "error: dependency closure contains $ACTUAL_CLASS_COUNT classes, expected $EXPECTED_CLASS_COUNT" >&2
    exit 1
fi

"$JAVA_BIN" -m jdk.jartool/sun.tools.jar.Main \
    --create --file "$BUILT_JAR" --date="$JAR_TIMESTAMP" \
    -C "$BUILD_DIR/classes" .

ACTUAL_JAR_SHA256=$(python3 - "$BUILT_JAR" <<'PY'
import hashlib
import pathlib
import sys

print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
)
ACTUAL_JAR_SIZE=$(wc -c < "$BUILT_JAR" | tr -d ' ')
if [ "$ACTUAL_JAR_SIZE" != "$EXPECTED_JAR_SIZE" ]; then
    echo "error: reproducible JAR is $ACTUAL_JAR_SIZE bytes, expected $EXPECTED_JAR_SIZE" >&2
    exit 1
fi
if [ "$ACTUAL_JAR_SHA256" != "$EXPECTED_JAR_SHA256" ]; then
    echo "error: non-reproducible SE 1.2.1 oracle JAR: $ACTUAL_JAR_SHA256" >&2
    exit 1
fi

cp "$BUILT_JAR" "$OUTPUT_JAR"

echo "built $OUTPUT_JAR"
echo "sha256 $ACTUAL_JAR_SHA256"
echo "classes $ACTUAL_CLASS_COUNT"
