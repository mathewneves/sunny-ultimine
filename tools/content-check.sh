#!/bin/sh
# Validates the content (assets/, data/) with the pinned Sunrise's own `cargo sunrise-mod build` and `check`, with no
# launcher: a copy of the manifest without [native] and the two folders, built into a .smod. Stops at the first error.
# CARGO_SUNRISE_MOD=<binary> picks the CLI (default: cargo-sunrise-mod on PATH, else one built from $SUNRISE_SRC).
set -eu
cd "$(dirname "$0")/.."
root="${SUNRISE_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/sunrise}"
cache="$root/sunny-ultimine"
cli="${CARGO_SUNRISE_MOD:-$(command -v cargo-sunrise-mod || echo "$cache/cli-target/release/cargo-sunrise-mod")}"
if [ ! -x "$cli" ] && [ -z "${CARGO_SUNRISE_MOD:-}" ]; then
  F="${SUNRISE_SRC:-$root/src}"
  [ -d "$F" ] || { echo "content-check: no cargo-sunrise-mod on PATH and no Sunrise source at $F (set SUNRISE_SRC)"; exit 1; }
  (cd "$F" && cargo build --release --locked -p cargo-sunrise-mod --target-dir "$cache/cli-target")
fi
[ -x "$cli" ] || { echo "content-check: no Sunrise CLI at $cli"; exit 1; }
d="$(mktemp -d)"; trap 'rm -rf "$d"' EXIT
cp -r sunrise-mod.toml assets data "$d"/
sed -i '/^\[native\]/,$d' "$d/sunrise-mod.toml"
(cd "$d" && "$cli" sunrise-mod build && "$cli" sunrise-mod check target/sunrise-mod/sunny_ultimine.smod)
