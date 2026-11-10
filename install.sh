#!/bin/sh
# Install an hbots tarball into $HBOTS_HOME/runtime/versions/<version>.
# `current` points at the version just installed. The previous `current` becomes `previous`.
set -eu

home=${HBOTS_HOME:-$HOME/.hbots}
runtime=$home/runtime
version=
archive=
sums=

usage() {
  echo "usage: install.sh [--version <version>]" >&2
  echo "usage: install.sh --version <version> --from <tarball> --sums <SHA256SUMS>" >&2
  echo "       HBOTS_RELEASE_BASE=<url> install.sh --version <version>" >&2
  exit 2
}

while [ $# -gt 0 ]; do
  case $1 in
    --version) version=${2:-}; shift 2 ;;
    --from) archive=${2:-}; shift 2 ;;
    --sums) sums=${2:-}; shift 2 ;;
    -h|--help) usage ;;
    *) echo "install.sh: unknown argument $1" >&2; usage ;;
  esac
done

if [ -z "$version" ]; then
  if [ -n "$archive" ] || [ -n "$sums" ] || [ -n "${HBOTS_RELEASE_BASE:-}" ]; then
    echo "install.sh: --version is required with --from, --sums, or HBOTS_RELEASE_BASE" >&2
    exit 1
  fi
  latest=$(curl -fsSL -o /dev/null -w '%{url_effective}' https://github.com/Herklos/Bots/releases/latest)
  case "$latest" in
    https://github.com/Herklos/Bots/releases/tag/v*) version=${latest##*/} ;;
    *) echo "install.sh: could not resolve the latest release: $latest" >&2; exit 1 ;;
  esac
fi
version=${version#v}
case "$version" in
  ''|*[!a-zA-Z0-9.+-]*) echo "install.sh: invalid version: $version" >&2; exit 1 ;;
esac

os=$(uname -s)
mach=$(uname -m)
case "$os-$mach" in
  Linux-x86_64) triple=x86_64-unknown-linux-gnu ;;
  Linux-aarch64|Linux-arm64) triple=aarch64-unknown-linux-gnu ;;
  Darwin-arm64) triple=aarch64-apple-darwin ;;
  Darwin-x86_64) triple=x86_64-apple-darwin ;;
  *) echo "install.sh: unsupported platform $os-$mach" >&2; exit 1 ;;
esac

name="hbots-${version}-${triple}.tar.gz"
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

if [ -z "$archive" ]; then
  base=${HBOTS_RELEASE_BASE:-https://github.com/Herklos/Bots/releases/download/v$version}
  base=${base%/}
  archive=$workdir/$name
  sums=$workdir/SHA256SUMS
  curl -fsSL "$base/$name" -o "$archive"
  curl -fsSL "$base/SHA256SUMS" -o "$sums"
fi

if [ ! -f "$archive" ] || [ ! -f "$sums" ]; then
  echo "install.sh: need the tarball and SHA256SUMS" >&2
  exit 1
fi

base_name=$(basename "$archive")
want=$(awk -v f="$base_name" '$2 == f { print $1; exit }' "$sums")
if [ -z "$want" ]; then
  echo "install.sh: $base_name is not listed in $sums" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$archive" | awk '{ print $1 }')
else
  got=$(shasum -a 256 "$archive" | awk '{ print $1 }')
fi
if [ "$got" != "$want" ]; then
  echo "install.sh: checksum mismatch for $base_name" >&2
  exit 1
fi

dest=$runtime/versions/$version
mkdir -p "$runtime/versions"
rm -rf "$dest"
mkdir -p "$dest"
tar -xzf "$archive" -C "$dest"
if [ ! -x "$dest/hbots" ]; then
  echo "install.sh: tarball has no hbots binary" >&2
  exit 1
fi

if [ -L "$runtime/current" ]; then
  ln -sfn "$(readlink "$runtime/current")" "$runtime/previous"
fi
ln -sfn "versions/$version" "$runtime/current"
echo "$runtime/current/hbots"
