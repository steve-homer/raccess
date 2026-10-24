#!/usr/bin/env bash
# Vendor test databases into fixtures/. Neither corpus is committed (~82MB
# total); run this once after cloning. Both upstreams are permissively
# licensed, so redistribution is fine — their LICENSE files are copied along.
#
#   fixtures/jackcess/  85 databases, every format version, but no forms/VBA/macros
#   fixtures/jetdb/     smaller, and the only source of forms, VBA and macros
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# --- Jackcess: broad format-version coverage ------------------------------
echo "Cloning jackcess ..."
git clone --depth 1 --quiet https://github.com/jahlborn/jackcess.git "$WORK/jackcess"

DEST="$ROOT/fixtures/jackcess"
echo "Copying Jackcess databases to $DEST ..."
rm -rf "$DEST"
mkdir -p "$DEST"
cp -r "$WORK/jackcess/src/test/data/." "$DEST/"
cp "$WORK/jackcess/LICENSE.txt" "$WORK/jackcess/NOTICE.txt" "$DEST/"
git -C "$WORK/jackcess" rev-parse HEAD >"$DEST/PROVENANCE-commit.txt"

# --- jetdb: the only corpus with forms, VBA and macros --------------------
echo "Cloning jetdb ..."
git clone --depth 1 --quiet https://github.com/dominion525/jetdb.git "$WORK/jetdb"

DEST="$ROOT/fixtures/jetdb"
echo "Copying jetdb databases to $DEST ..."
rm -rf "$DEST"
mkdir -p "$DEST"
cp -r "$WORK/jetdb/testdata/." "$DEST/"
cp "$WORK/jetdb/LICENSE-MIT" "$WORK/jetdb/LICENSE-APACHE" "$DEST/"
git -C "$WORK/jetdb" rev-parse HEAD >"$DEST/PROVENANCE-commit.txt"

total=$(find "$ROOT/fixtures" -type f \( -iname '*.mdb' -o -iname '*.accdb' \) | wc -l)
echo "Done: $total databases under $ROOT/fixtures"
