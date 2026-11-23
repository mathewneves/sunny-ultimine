#!/bin/sh
# Builds the mod into a Sunrise launcher instance (SUNNY_ULTIMINE_INSTANCE, default sunny-ultimine).
# Usage: tools/build.sh [--dev]
# --dev builds a scratch copy whose Cargo.toml turns the `dev` feature on by default (the launcher's native-build takes
# no --features), so the build has the /sunny_ultimine_dev console commands tools/server-check.sh scenarios use.
# SUNRISE_LAUNCHER=<binary> picks the launcher (default: sunrise-launcher on PATH).
set -eu
repo="$(cd "$(dirname "$0")/.." && pwd)"
launcher="${SUNRISE_LAUNCHER:-$(command -v sunrise-launcher || echo sunrise-launcher)}"
[ -x "$launcher" ] || { echo "build: no Sunrise launcher at $launcher (set SUNRISE_LAUNCHER)"; exit 1; }
cache="${SUNRISE_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/sunrise}/sunny-ultimine"
mkdir -p "$cache"
src="$repo"
if [ "${1:-}" = --dev ]; then
  src="$cache/dev-build"; mkdir -p "$src"
  rsync -a --delete --exclude target --exclude .git --exclude .sunrise --exclude src/core/target "$repo"/ "$src"/
  grep -q '^dev *=' "$src/Cargo.toml" || { echo "build: Cargo.toml has no dev feature"; exit 1; }
  ! grep -q '^default *=' "$src/Cargo.toml" || { echo "build: Cargo.toml already sets default features"; exit 1; }
  sed -i '/^\[features\]/a default = ["dev"]' "$src/Cargo.toml"
fi
# native-lock refuses to replace an existing Cargo.lock, so it runs only when the lock is missing
# (move pins on purpose with `<launcher> native-lock . --update-lock`).
exec flock "$cache/build.lock" sh -c '{ [ -f "$2/Cargo.lock" ] || "$1" native-lock "$2"; } && exec "$1" native-build "$2" --instance "$3" --trust-native-code' \
  sh "$launcher" "$src" "${SUNNY_ULTIMINE_INSTANCE:-sunny-ultimine}"
