#!/bin/sh
# Pack the binary and web app into a release archive and record its SHA-256.
set -eu

root=$(CDPATH='' cd -- "$(dirname "$0")/.." && pwd)
version=${HBOTS_VERSION:-}
if [ -z "$version" ]; then
  version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$root/Cargo.toml" | head -n 1)
fi
version=${version#v}
if [ -z "$version" ]; then
  echo "pack-release: set HBOTS_VERSION or version in Cargo.toml" >&2
  exit 1
fi

if [ -n "${HBOTS_TARGET:-}" ]; then
  target=$HBOTS_TARGET
else
  target=$(rustc -vV | sed -n 's/^host: //p')
fi
if [ -z "$target" ]; then
  echo "pack-release: could not tell the target triple" >&2
  exit 1
fi

bin=${HBOTS_BIN:-$root/target/release/hbots}
if [ ! -f "$bin" ]; then
  echo "pack-release: no binary at $bin (cargo build --release first)" >&2
  exit 1
fi

static_dir=${STATIC_DIR:-$root/app/target/web-dist}
if [ ! -s "$static_dir/index.html" ]; then
  echo "pack-release: no web app at $static_dir/index.html; build it first or set STATIC_DIR" >&2
  exit 1
fi

name="hbots-${version}-${target}"
outdir=${HBOTS_DIST:-$root/dist}
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

cp "$bin" "$stage/hbots"
chmod 755 "$stage/hbots"
mkdir -p "$stage/web"
cp -R "$static_dir/." "$stage/web/"

mkdir -p "$outdir"
tar -C "$stage" -czf "$outdir/$name.tar.gz" hbots web
sums="$outdir/SHA256SUMS"
tmp=$(mktemp)
if [ -f "$sums" ]; then
  grep -v "  $name.tar.gz$" "$sums" > "$tmp" || true
fi
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$outdir" && sha256sum "$name.tar.gz") >> "$tmp"
else
  (cd "$outdir" && shasum -a 256 "$name.tar.gz") >> "$tmp"
fi
mv "$tmp" "$sums"
echo "$outdir/$name.tar.gz"
