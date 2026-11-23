#!/bin/sh
# Builds the mod into a Sunrise launcher instance (SUNNY_ULTIMINE_INSTANCE, default sunny-ultimine).
# SUNRISE_LAUNCHER=<binary> picks the launcher (default: sunrise-launcher on PATH).
set -eu
repo="$(cd "$(dirname "$0")/.." && pwd)"
launcher="${SUNRISE_LAUNCHER:-$(command -v sunrise-launcher || echo sunrise-launcher)}"
[ -x "$launcher" ] || { echo "build: no Sunrise launcher at $launcher (set SUNRISE_LAUNCHER)"; exit 1; }
cache="${SUNRISE_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/sunrise}/sunny-ultimine"
mkdir -p "$cache"
# native-lock refuses to replace an existing Cargo.lock, so it runs only when the lock is missing
# (move pins on purpose with `<launcher> native-lock . --update-lock`).
exec flock "$cache/build.lock" sh -c '{ [ -f "$2/Cargo.lock" ] || "$1" native-lock "$2"; } && exec "$1" native-build "$2" --instance "$3" --trust-native-code' \
  sh "$launcher" "$repo" "${SUNNY_ULTIMINE_INSTANCE:-sunny-ultimine}"
