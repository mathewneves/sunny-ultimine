#!/bin/sh
# An offscreen single-player client of the sunny-ultimine instance's current build, driven by its sunrise-ctl.
# Saves, logs and options.txt stay in $SUNRISE_CACHE_DIR/sunny-ultimine/client-home (muted via soundCategory_master:0.0).
# Usage: tools/client.sh start [world [game args...]]
#        tools/client.sh <sunrise-ctl command ...>
#        tools/client.sh stop
set -eu
data="${SUNRISE_DATA:-${XDG_DATA_HOME:-$HOME/.local/share}/sunrise/launcher}"
build="${SUNNY_ULTIMINE_BUILD:-$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["instances"][sys.argv[2]]["version"])' "$data/instances.json" "${SUNNY_ULTIMINE_INSTANCE:-sunny-ultimine}")}"
pin="$(sed -n 's/^sunrise = "\(.......\).*"/\1/p' "$(dirname "$0")/../sunrise-mod.toml")"
base="$(ls -d "$data"/versions/nightly-*-"$pin" | tail -1)"
ctl="$data/versions/$build/sunrise-ctl"
cache="${SUNRISE_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/sunrise}/sunny-ultimine"
sock="${TMPDIR:-/tmp}/sunny-ultimine-client${SUNNY_ULTIMINE_CLIENT:+-$SUNNY_ULTIMINE_CLIENT}.sock"
SUNRISE_HOME="$cache/client-home${SUNNY_ULTIMINE_CLIENT:+-$SUNNY_ULTIMINE_CLIENT}"; export SUNRISE_HOME
mkdir -p "$SUNRISE_HOME" "$(dirname "$sock")"
case "${1:-}" in
  start)
    shift; world="${1:-sunny-ultimine-test}"; [ $# = 0 ] || shift
    rm -f "$sock"; cd "$SUNRISE_HOME"
    # Always mute master volume before launching.
    sed -i '/^soundCategory_master:/d' options.txt 2>/dev/null || :; echo soundCategory_master:0.0 >> options.txt
    exec env -u WAYLAND_DISPLAY "$ctl" --socket "$sock" --timeout 180 launch --world "$world" \
      --creative --seed 1 --wait --offscreen --resources "$base/.local/sunrise-client/26.3" "$@" > /dev/null ;;
  stop) exec "$ctl" --socket "$sock" quit ;;
  '') echo "usage: tools/client.sh start [world] | stop | <sunrise-ctl command ...>"; exit 2 ;;
  help) exec "$ctl" --help ;;
  *) exec "$ctl" --socket "$sock" "$@" ;;
esac
