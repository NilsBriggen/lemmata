#!/usr/bin/env bash
# Starts a test build of Lemmata with the automation bridge, its own
# settings folders and a fresh copy of the article fixture. Development only.
#
#   scripts/automation/launch.sh          # start (build first with --build)
#   scripts/automation/launch.sh --build
# The fixture copy is at $LEMMATA_TEST_DIR/article (default /tmp/lemmata-test).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
dir=${LEMMATA_TEST_DIR:-/tmp/lemmata-test}

if [[ ${1:-} == --build ]]; then
  VITE_LEMMATA_TEST=1 pnpm --dir "$root/apps/desktop" tauri build --debug --no-bundle 2>&1 | tail -1
fi
pkill -x lemmata-app 2>/dev/null && sleep 1 || true
rm -rf "$dir"
mkdir -p "$dir"/{config,data,cache}
cp -r "$root/tests/fixtures/article" "$dir/article"
XDG_CONFIG_HOME="$dir/config" XDG_DATA_HOME="$dir/data" XDG_CACHE_HOME="$dir/cache" LEMMATA_AUTOMATION=1 \
  nohup "$root/target/debug/lemmata-app" >"$dir/app.log" 2>&1 &
for _ in $(seq 50); do
  node "$root/scripts/automation/drive.mjs" state >/dev/null 2>&1 && break
  sleep 0.2
done
node "$root/scripts/automation/drive.mjs" openProject "{\"path\":\"$dir/article\"}" >/dev/null
node "$root/scripts/automation/drive.mjs" idle '{"timeout":90000}' >/dev/null
echo "Lemmata is running with $dir/article open."
