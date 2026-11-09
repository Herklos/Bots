#!/bin/sh
# Install Bots:  curl -fsSL https://raw.githubusercontent.com/Herklos/Bots/main/install.sh | sh
#
# Downloads a release for this machine, checks it against the release's SHA256SUMS, and installs it in
# $BOTS_HOME/runtime/versions/<version>, with `current` pointing at it (the one it replaces stays as `previous`),
# like Threebot's installer. Bots needs no Node.js: the release is one binary plus the built web app.
#
# Settings (environment variables):
#   BOTS_VERSION           exact version to install (default: newest release)
#   BOTS_HOME              data and installs (default: $THREEBOT_HOME, else ~/.threebot: the same home Threebot uses)
#   BOTS_INSTALL_BIN_DIR   where the `bots` command goes (default: ~/.local/bin)
#   BOTS_RELEASE_BASE_URL  download mirror; files are read from <base>/v<version>/
set -eu

REPO="Herklos/Bots"
HOME_DIR="${BOTS_HOME:-${THREEBOT_HOME:-$HOME/.threebot}}"
BIN_DIR="${BOTS_INSTALL_BIN_DIR:-$HOME/.local/bin}"
BASE_URL="${BOTS_RELEASE_BASE_URL:-https://github.com/$REPO/releases/download}"

say() { printf '%s\n' "$*"; }
die() { printf 'bots install: %s\n' "$*" >&2; exit 1; }

fetch() { # fetch <url> <file>
  if command -v curl >/dev/null 2>&1; then curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then wget -q -O "$2" "$1"
  else die "needs curl or wget"; fi
}
sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  else die "needs sha256sum or shasum"; fi
}

# The release target triple for this machine. Only the targets release.yml builds are listed.
target() {
  os="$(uname -s)"; arch="$(uname -m)"
  case "$os" in
    Linux)
      case "$arch" in
        x86_64|amd64) echo x86_64-unknown-linux-gnu ;;
        aarch64|arm64) echo aarch64-unknown-linux-gnu ;;
        *) die "no Bots release for Linux $arch" ;;
      esac ;;
    Darwin)
      case "$arch" in
        arm64|aarch64) echo aarch64-apple-darwin ;;
        x86_64) echo x86_64-apple-darwin ;;
        *) die "no Bots release for macOS $arch" ;;
      esac ;;
    *) die "no Bots release for $os (Windows is not supported)" ;;
  esac
}

command -v tar >/dev/null 2>&1 || die "needs tar"
TARGET="$(target)"

VERSION="${BOTS_VERSION:-}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM
if [ -z "$VERSION" ]; then
  fetch "https://api.github.com/repos/$REPO/releases/latest" "$TMP/latest.json" || die "couldn't find the latest release"
  VERSION="$(sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' "$TMP/latest.json" | head -n 1)"
  [ -n "$VERSION" ] || die "couldn't read the latest release"
fi
VERSION="${VERSION#v}"
NAME="bots-$VERSION-$TARGET"
ARCHIVE="$NAME.tar.gz"

say "Downloading Bots $VERSION for $TARGET"
fetch "$BASE_URL/v$VERSION/$ARCHIVE" "$TMP/$ARCHIVE" || die "download failed: $BASE_URL/v$VERSION/$ARCHIVE"
fetch "$BASE_URL/v$VERSION/SHA256SUMS" "$TMP/SHA256SUMS" || die "download failed: SHA256SUMS"
EXPECTED="$(awk -v f="$ARCHIVE" '$2==f || $2=="*"f {print $1}' "$TMP/SHA256SUMS")"
[ -n "$EXPECTED" ] || die "SHA256SUMS has no entry for $ARCHIVE"
ACTUAL="$(sha256 "$TMP/$ARCHIVE")"
[ "$EXPECTED" = "$ACTUAL" ] || die "checksum mismatch for $ARCHIVE (expected $EXPECTED, got $ACTUAL)"

VERSIONS="$HOME_DIR/runtime/versions"
TARGET_DIR="$VERSIONS/$VERSION"
mkdir -p "$VERSIONS"
if [ ! -f "$TARGET_DIR/.install-complete" ]; then
  STAGING="$(mktemp -d "$VERSIONS/.staging-XXXXXX")"
  tar -xzf "$TMP/$ARCHIVE" -C "$STAGING" --strip-components=1
  # The binary must run and report the version it was released as, before it replaces anything.
  REPORTED="$("$STAGING/bots" --version | awk '{print $2}')" || { rm -rf "$STAGING"; die "the downloaded bots does not run"; }
  [ "$REPORTED" = "$VERSION" ] || { rm -rf "$STAGING"; die "the downloaded bots reports $REPORTED, not $VERSION"; }
  date > "$STAGING/.install-complete"
  rm -rf "$TARGET_DIR"
  mv "$STAGING" "$TARGET_DIR"
fi

# `current` is the version the service runs; the one it replaces stays as `previous`.
RUNTIME="$HOME_DIR/runtime"
if [ -L "$RUNTIME/current" ] && [ "$(readlink "$RUNTIME/current")" != "$TARGET_DIR" ]; then
  ln -sfn "$(readlink "$RUNTIME/current")" "$RUNTIME/previous"
fi
ln -sfn "$TARGET_DIR" "$RUNTIME/current"

mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/bots" <<EOF
#!/bin/sh
# Written by the Bots installer: runs the installed version with its web app.
THREEBOT_HOME="\${THREEBOT_HOME:-$HOME_DIR}" STATIC_DIR="\${STATIC_DIR:-$RUNTIME/current/web}" exec "$RUNTIME/current/bots" "\$@"
EOF
chmod 755 "$BIN_DIR/bots"

say ""
say "Bots $VERSION is installed in $TARGET_DIR."
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) say "Add $BIN_DIR to your PATH, or run $BIN_DIR/bots directly." ;; esac
say "Run \`bots --version\` to check it."
