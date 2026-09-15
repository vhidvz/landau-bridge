#!/usr/bin/env sh
# Screenshots every page (dark only) through the dev fixture mode (app/dev/mockBridge.ts).
# Run `pnpm dev` first. Usage: scripts/shots.sh OUT_DIR [page-filter] [--grid]
# SHOT_HEIGHT=1700 scripts/shots.sh ... captures a taller viewport (<main> scrolls internally).
set -eu
out=${1:?output directory}
filter=${2:-}
grid=""
for a in "$@"; do [ "$a" = "--grid" ] && grid="&grid=1"; done
[ "$filter" = "--grid" ] && filter=""
mkdir -p "$out"
height=${SHOT_HEIGHT:-800}
pages="/?state=done /?state=empty /?state=running /?state=review /?state=done&appimage=1 /import?state=running /import?state=review /import?state=done /import?state=done&job=42 /refresh?state=done /refresh?state=refreshing /matches?state=done /matches?state=empty /jobs?state=done /jobs/42?state=done /jobs/40?state=done /jobs/43?state=running /errors?state=done /errors?state=done&job=42 /settings?state=done /settings?state=empty /?state=done&fx=lite"
for p in $pages; do
  case "$p" in *"$filter"*) ;; *) continue ;; esac
  name=$(printf '%s' "$p" | tr '/?=&' '____' | sed 's/^_//; s/^$/root/')
  [ -n "$grid" ] && name="$name-grid"
  fx=""
  case "$p" in *fx=*) ;; *) fx="&fx=full" ;; esac
  google-chrome-stable --headless=new --no-proxy-server --disable-gpu --hide-scrollbars --no-sandbox \
    --window-size=1200,"$height" --virtual-time-budget=6000 --force-device-scale-factor=1 --force-prefers-reduced-motion \
    --screenshot="$out/$name.png" "http://localhost:3210$p$fx$grid" >/dev/null 2>&1
  echo "$out/$name.png"
done
