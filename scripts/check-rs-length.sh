#!/bin/sh
# Fails when a Rust file is longer than MAX lines. Split by concern instead (see AGENTS.md).
set -eu
MAX="${RS_MAX_LINES:-500}"
cd "$(dirname "$0")/.."
bad=0
for f in $(find . -name '*.rs' -not -path './target/*' -not -path './node_modules/*' -not -path './app/target/*'); do
  n=$(wc -l < "$f" | tr -d ' ')
  if [ "$n" -gt "$MAX" ]; then
    echo "$f has $n lines (max $MAX)"
    bad=1
  fi
done
exit "$bad"
