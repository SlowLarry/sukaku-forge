#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUTPUT_JAR=${1:-"$ROOT/target/sudokumonster/SukakuExplainer-v1.18.1.jar"}
ASSET_URL=https://github.com/SudokuMonster/SukakuExplainer/releases/download/v1.18.1/SukakuExplainer.jar
EXPECTED_SHA256=37831647bf1727be02c159f25aefd8602918185d79d4aee73472eff40cd6736c
EXPECTED_SIZE=636042

DOWNLOAD_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sudokumonster-v118.XXXXXX")
trap 'rm -rf "$DOWNLOAD_DIR"' EXIT HUP INT TERM
DOWNLOADED_JAR="$DOWNLOAD_DIR/SukakuExplainer.jar"

curl --fail --location --retry 3 --silent --show-error \
    --output "$DOWNLOADED_JAR" "$ASSET_URL"

ACTUAL_SIZE=$(wc -c < "$DOWNLOADED_JAR" | tr -d ' ')
ACTUAL_SHA256=$(sha256sum "$DOWNLOADED_JAR" | awk '{ print $1 }')
if [ "$ACTUAL_SIZE" != "$EXPECTED_SIZE" ]; then
    echo "error: SudokuMonster v1.18.1 JAR is $ACTUAL_SIZE bytes, expected $EXPECTED_SIZE" >&2
    exit 1
fi
if [ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]; then
    echo "error: SudokuMonster v1.18.1 JAR SHA-256 is $ACTUAL_SHA256, expected $EXPECTED_SHA256" >&2
    exit 1
fi

mkdir -p "$(dirname -- "$OUTPUT_JAR")"
cp "$DOWNLOADED_JAR" "$OUTPUT_JAR"

echo "downloaded $OUTPUT_JAR"
echo "sha256 $ACTUAL_SHA256"
