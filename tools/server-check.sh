#!/bin/sh
# Headless check: run the sunny-ultimine instance's build (Sunrise launcher) as a dedicated server in a scratch folder,
# send console commands, then stop. Never touches a real game folder. The native build holds the mod's code and content
# (its sunrise-native-mods/sunny_ultimine), so nothing is installed here.
# Scenarios drive players with /sunny_ultimine_dev, which only a `dev` feature build has: run `tools/build.sh --dev` first.
# Usage: tools/server-check.sh "setblock 0 100 0 minecraft:iron_ore" ["more commands" ...]
#    or: tools/server-check.sh -f tools/scenarios/<name>.txt
# Commands are sent SUNNY_ULTIMINE_PACE seconds apart (default 0.1: two game ticks), once the server logs "Done ("; a
# "wait" line sends nothing and holds one second. A "pace <seconds>" line in a scenario sets its own spacing.
# A scenario file holds one command per line, "wait" lines, "#" comments and "expect <extended regex>" lines.
# Each expect must match a line of the server log after the run, or the script exits 1 naming it.
set -eu
# SUNNY_ULTIMINE_INSTANCE picks the build (default sunny-ultimine); SUNRISE_LAUNCHER the launcher (default: on PATH).
instance="${SUNNY_ULTIMINE_INSTANCE:-sunny-ultimine}"
launcher="${SUNRISE_LAUNCHER:-$(command -v sunrise-launcher || echo sunrise-launcher)}"
[ -x "$launcher" ] || { echo "server-check: no Sunrise launcher at $launcher (set SUNRISE_LAUNCHER)"; exit 1; }
data="${SUNRISE_DATA:-${XDG_DATA_HOME:-$HOME/.local/share}/sunrise/launcher}"
cache="${SUNRISE_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/sunrise}/sunny-ultimine"
# One Python process resolves the build and asks the system for a free port.
# SUNNY_ULTIMINE_BUILD=<version folder name> runs that build instead.
build_and_port="$(python3 -c '
import socket
import sys

build, data, instance = sys.argv[1:]
if not build:
    import json
    with open(f"{data}/instances.json") as state:
        build = json.load(state)["instances"][instance]["version"]
with socket.socket() as sock:
    sock.bind(("", 0))
    print(f"{build} {sock.getsockname()[1]}")
' "${SUNNY_ULTIMINE_BUILD:-}" "$data" "$instance")"
build="${build_and_port% *}"
port="${build_and_port##* }"
[ -x "$data/versions/$build/sunrise-server" ] || { echo "no $instance build ($build)"; exit 1; }
# Dev-only console commands stand in for a player's input here.
export SUNNY_ULTIMINE_DEV=1
# SUNNY_ULTIMINE_CHECK_DIR=<folder> keeps the world between runs (save/reload checks); otherwise a temp folder.
# The scratch folder is on disk (the cache dir), not /tmp, which is often RAM.
runs="$cache/runs"; mkdir -p "$runs"
if [ -n "${SUNNY_ULTIMINE_CHECK_DIR:-}" ]; then dir="$SUNNY_ULTIMINE_CHECK_DIR"; else dir="$(mktemp -d -p "$runs")"; trap 'rm -rf "$dir"' EXIT; fi
mkdir -p "$dir"
: > "$dir/commands"; : > "$dir/expects"
if [ "${1:-}" = -f ]; then
  [ -f "${2:-}" ] || { echo "server-check: no scenario file '${2:-}'"; exit 2; }
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      ''|'#'*) ;;
      'expect '*) printf '%s\n' "${line#expect }" >> "$dir/expects" ;;
      pace|'pace '*) SUNNY_ULTIMINE_PACE="${line#pace}"; SUNNY_ULTIMINE_PACE="${SUNNY_ULTIMINE_PACE# }"
        case "$SUNNY_ULTIMINE_PACE" in ''|.|*[!0-9.]*|*.*.*) echo "server-check: bad pace line '$line'"; exit 2 ;; esac ;;
      *) printf '%s\n' "$line" >> "$dir/commands" ;;
    esac
  done < "$2"
else
  for c in "$@"; do printf '%s\n' "$c" >> "$dir/commands"; done
fi
# A bad pace would only make each sleep fail while the run went on.
case "${SUNNY_ULTIMINE_PACE:-0.1}" in ''|.|*[!0-9.]*|*.*.*) echo "server-check: bad pace '${SUNNY_ULTIMINE_PACE}'"; exit 2 ;; esac
mkdir -p "$dir/game"
echo "eula=true" > "$dir/game/eula.txt"
printf 'server-port=%s\nonline-mode=false\nlevel-seed=1\npause-when-empty-seconds=0\n' "$port" > "$dir/game/server.properties"
log="$dir/server.log"
: > "$log"
# A machine-wide cap on servers (SUNNY_ULTIMINE_SLOTS, default 4, about 1.7 GB each).
# The slot's lock is held by fd 9 until this script exits.
until for n in $(seq 1 "${SUNNY_ULTIMINE_SLOTS:-4}"); do exec 9> "$cache/server.$n.lock"; flock -n 9 && break; done; do sleep 2; done
cd "$dir/game"
status=0
{
  # Up to 120 s for "Done (", then stop anyway (the check below fails the run).
  i=0; until grep -q 'Done (' "$log" || [ "$i" -ge 1200 ]; do sleep 0.1; i=$((i + 1)); done
  # `forceload add` loads the chunk synchronously before returning.
  echo "forceload add 0 0"; sleep "${SUNNY_ULTIMINE_PACE:-0.1}"
  while IFS= read -r c; do if [ "$c" = wait ]; then sleep 1; else echo "$c"; sleep "${SUNNY_ULTIMINE_PACE:-0.1}"; fi; done < "$dir/commands"
  echo stop
} | timeout 300 "$launcher" server --dir "$dir/game" --version "$build" --accept-eula \
  > "$log" 2>&1 || status=$?
grep -v "OFFLINE\|authenticate\|internet access\|online-mode\|Saving chunks" "$log" || true
fail=0
grep -q 'Done (' "$log" || { echo "server-check: FAIL: the server didn't start"; fail=1; }
[ "$status" -eq 0 ] || { echo "server-check: FAIL: the server exited with $status"; fail=1; }
while IFS= read -r re; do
  grep -Eq -- "$re" "$log" || { echo "server-check: FAIL: expect $re"; fail=1; }
done < "$dir/expects"
grep -q 'sunny_ultimine_dev' "$dir/commands" && grep -q 'Unknown or incomplete command' "$log" &&
  echo "server-check: hint: is the build missing the dev feature? Run tools/build.sh --dev"
exit "$fail"
