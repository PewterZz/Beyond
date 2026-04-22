#!/usr/bin/env bash
# Build Beyondtty.app from a release binary + Info.plist template + icon.
#
# Usage:
#   scripts/bundle-macos-app.sh                 # builds locally with cargo
#   BIN=path/to/beyondtty scripts/bundle-macos-app.sh   # use prebuilt binary
#
# Output: ./Beyondtty.app (standalone bundle; move to /Applications to install).
#
# The Info.plist contains NSAppSleepDisabled=true so macOS doesn't throttle the
# app when its window isn't frontmost — without it, streaming agent output and
# PTY I/O get QoS-downgraded under App Nap, which surfaces as "commands spin,
# agent responses crawl" compared to the same binary launched from a terminal.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$REPO_ROOT/Beyondtty.app"
PLIST_TEMPLATE="$REPO_ROOT/macos/Info.plist.template"
ICNS="$REPO_ROOT/assets/beyondtty.icns"

VERSION="$(awk -F'"' '/^version = /{print $2; exit}' "$REPO_ROOT/Cargo.toml" 2>/dev/null || true)"
if [ -z "${VERSION:-}" ]; then
    VERSION="$(awk -F'"' '/^version\.workspace/{print; exit}' "$REPO_ROOT/Cargo.toml" >/dev/null 2>&1 \
        && awk -F'"' '/^version = /{print $2; exit}' "$REPO_ROOT/Cargo.toml")"
fi
: "${VERSION:=0.0.0}"

if [ -n "${BIN:-}" ]; then
    [ -x "$BIN" ] || { echo "error: BIN=$BIN is not executable" >&2; exit 1; }
else
    (cd "$REPO_ROOT" && cargo build --release --bin beyondtty)
    BIN="$REPO_ROOT/target/release/beyondtty"
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# Install the real binary as beyondtty-bin and a thin wrapper as the
# CFBundleExecutable entry point. The wrapper redirects stderr to
# ~/Library/Logs/Beyondtty/beyondtty.log so RUST_LOG output (set via
# `launchctl setenv RUST_LOG ...`) survives the open -a launch path
# where the process has no controlling terminal.
cp "$BIN" "$APP/Contents/MacOS/beyondtty-bin"
chmod +x "$APP/Contents/MacOS/beyondtty-bin"
cat > "$APP/Contents/MacOS/beyondtty" << 'WRAPPER'
#!/bin/sh
LOG="$HOME/Library/Logs/Beyondtty/beyondtty.log"
mkdir -p "$(dirname "$LOG")"
exec "$(dirname "$0")/beyondtty-bin" 2>>"$LOG"
WRAPPER
chmod +x "$APP/Contents/MacOS/beyondtty"

cp "$ICNS" "$APP/Contents/Resources/beyondtty.icns"

sed "s/@VERSION@/$VERSION/g" "$PLIST_TEMPLATE" > "$APP/Contents/Info.plist"

codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || codesign --force --deep --sign - "$APP"
xattr -cr "$APP"

echo "Built $APP (v$VERSION)"
