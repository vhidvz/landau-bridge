#!/usr/bin/env sh
# Screenshots every page in both colour modes through the dev fixture mode (app/dev/mockBridge.ts).
# Run `pnpm dev` first. Usage: scripts/shots.sh OUT_DIR [page-filter]
set -eu
out=${1:?output directory}
filter=${2:-}
mkdir -p "$out"
pages="/?state=done /?state=empty /import?state=running /import?state=review /import?state=done /refresh?state=done /refresh?state=refreshing /matches?state=done /matches?state=empty /logs?state=done /logs?state=done&job=42 /settings?state=done"
for mode in light dark; do
  for p in $pages; do
    case "$p" in *"$filter"*) ;; *) continue ;; esac
    name=$(printf '%s' "$p" | tr '/?=&' '____' | sed 's/^_//; s/^$/root/')
    google-chrome-stable --headless=new --no-proxy-server --disable-gpu --hide-scrollbars --no-sandbox \
      --window-size=1200,800 --virtual-time-budget=6000 --force-device-scale-factor=1 --force-prefers-reduced-motion \
      --screenshot="$out/$mode-$name.png" "http://localhost:3210$p&mode=$mode" >/dev/null 2>&1
    echo "$out/$mode-$name.png"
  done
done
