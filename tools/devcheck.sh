#!/bin/bash
# usage: tools/devcheck.sh [mod dir] [check|test|clippy|doc] [extra cargo args]
# Compiles the mod against the pinned Sunrise source ($SUNRISE_SRC) in a scratch copy, without the launcher.
# The dev commands need the `dev` feature: tools/devcheck.sh . check --features dev
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
if [ $# -gt 0 ] && [ -f "$1/sunrise-mod.toml" ]; then MOD=$(cd "$1" && pwd); shift; else MOD="$root"; fi
CMD=${1:-check}; shift || true
pin=$(sed -n 's/^sunrise = "\(.*\)"/\1/p' "$MOD/sunrise-mod.toml")
scache="${SUNRISE_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/sunrise}"
F="${SUNRISE_SRC:-$scache/src}"
[ -d "$F/crates/sunrise-mod-api" ] || { echo "devcheck: no Sunrise source at $F (set SUNRISE_SRC)"; exit 1; }
[ "$(git -C "$F" rev-parse HEAD)" = "$pin" ] || { echo "devcheck: $F is not at the pin $pin"; exit 1; }
cache="$scache/sunny-ultimine"
W=${DEVCHECK_WORK:-$cache/devcheck-work}
mkdir -p "$W"
rsync -a --delete --exclude target --exclude .git --exclude .sunrise --exclude data --exclude tools --exclude src/core/target --exclude src/core/Cargo.lock "$MOD"/ "$W"/
cp "$F/Cargo.lock" "$W/Cargo.lock"
sed -i "s#path = \".sunrise/sdk/api/crates/sunrise-mod-api\"#path = \"$F/crates/sunrise-mod-api\"#" "$W/Cargo.toml"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$cache/check-target}
cd "$W"
exec cargo "$CMD" "$@"
