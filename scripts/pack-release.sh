#!/bin/sh
# Packs one release archive for one target: bots-<version>-<target>.tar.gz, and appends its line to SHA256SUMS.
#
#   scripts/pack-release.sh --bin target/release/bots --web app/target/web-dist \
#       --target x86_64-unknown-linux-gnu [--version 0.1.0] [--out release]
#
# The archive holds one top-level folder, bots-<version>-<target>/, with:
#   bots          the binary
#   web/          the built Exact web app (the gateway serves it from STATIC_DIR)
#   LICENSE, NOTICE, README.md
# install.sh downloads these archives and checks them against SHA256SUMS.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN=""; WEB=""; TARGET=""; VERSION=""; OUT="$ROOT/release"

die() { printf 'pack-release: %s\n' "$*" >&2; exit 1; }
usage() { sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --bin) BIN="${2:-}"; shift 2 ;;
    --web) WEB="${2:-}"; shift 2 ;;
    --target) TARGET="${2:-}"; shift 2 ;;
    --version) VERSION="${2:-}"; shift 2 ;;
    --out) OUT="${2:-}"; shift 2 ;;
    -h|--help) usage ;;
    *) die "unknown argument: $1" ;;
  esac
done

[ -n "$BIN" ] || die "--bin is required"
[ -f "$BIN" ] || die "no binary at $BIN"
[ -n "$WEB" ] || die "--web is required"
[ -f "$WEB/index.html" ] || die "no web app at $WEB (expected index.html); build it first"
[ -n "$TARGET" ] || die "--target is required (e.g. x86_64-unknown-linux-gnu)"
case "$TARGET" in
  *[!A-Za-z0-9_.-]*|"") die "bad target: $TARGET" ;;
esac

if [ -z "$VERSION" ]; then
  # The workspace version in the root Cargo.toml is the release version.
  VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -n 1)"
fi
case "$VERSION" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) die "bad version: '$VERSION'" ;;
esac
case "$VERSION" in
  *[!A-Za-z0-9.+-]*) die "bad version: '$VERSION'" ;;
esac

# Run the binary to be sure it is the version we label it with.
REPORTED="$("$BIN" --version | awk '{print $2}')"
[ "$REPORTED" = "$VERSION" ] || die "binary reports $REPORTED but the release is $VERSION"

NAME="bots-$VERSION-$TARGET"
ARCHIVE="$NAME.tar.gz"
mkdir -p "$OUT"
STAGE="$(mktemp -d "${TMPDIR:-/tmp}/bots-pack.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT INT TERM

mkdir "$STAGE/$NAME"
cp "$BIN" "$STAGE/$NAME/bots"
chmod 755 "$STAGE/$NAME/bots"
cp -R "$WEB" "$STAGE/$NAME/web"
for f in LICENSE NOTICE README.md; do cp "$ROOT/$f" "$STAGE/$NAME/$f"; done

# COPYFILE_DISABLE keeps macOS tar from adding ._ metadata files.
COPYFILE_DISABLE=1 tar -czf "$OUT/$ARCHIVE" -C "$STAGE" "$NAME"

if command -v sha256sum >/dev/null 2>&1; then
  SUM="$(sha256sum "$OUT/$ARCHIVE" | cut -d' ' -f1)"
else
  SUM="$(shasum -a 256 "$OUT/$ARCHIVE" | cut -d' ' -f1)"
fi
# One line per archive; the publish job collects them all into one SHA256SUMS.
printf '%s  %s\n' "$SUM" "$ARCHIVE" >> "$OUT/SHA256SUMS"
printf '%s\n%s\n' "$OUT/$ARCHIVE" "$SUM"
