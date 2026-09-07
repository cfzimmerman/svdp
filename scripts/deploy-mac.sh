#!/usr/bin/env bash
# Build the release archive on the Mac and put it where it can be picked up.
#
# The archive is the single file a volunteer receives: extension, skills and
# instructions in one. See scripts/build-release.sh.
#
# Excludes are ANCHORED with a leading slash. An unanchored `servware/` also
# matches `src/servware/`, which once shipped a source tree missing the entire
# protocol layer and then deployed a stale bundle over the top of it.
#
# The remote half runs through `bash -s` rather than an inline `ssh "..."`
# string: the Mac's login shell is zsh, which makes a hard error of an unmatched
# glob, and nested quoting in an inline command is where this script kept
# breaking.
set -euo pipefail

HOST="${1:-mac}"
REMOTE="${2:-/Users/cory/Projects/svdp-spike}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "==> syncing to $HOST:$REMOTE"
rsync -az --delete \
  --exclude '/target/' --exclude '/.git/' --exclude '/dist/' \
  --exclude '/servware/' --exclude '/recordings/' \
  --exclude '.env' --exclude '*.csv' --exclude '*.har' --exclude '*.har2' \
  "$ROOT/" "$HOST:$REMOTE/"

# Guard: the sync must not have dropped source the build needs.
ssh -o BatchMode=yes "$HOST" "test -d '$REMOTE/src/servware' && test -d '$REMOTE/src/domain'" \
  || { echo "!! sync dropped source directories; aborting" >&2; exit 1; }

echo "==> building on $HOST"
# No SVDP_TOOLCHAIN: rust-toolchain.toml pins the version and rustup installs
# it on the Mac on first use. This used to hard-code 1.90.0, which was a third
# place the Rust version lived and had drifted from both CI and this machine.
ssh -o BatchMode=yes "$HOST" REMOTE="$REMOTE" \
    SVDP_UNIVERSAL=1 SVDP_PLATFORM=darwin bash -s <<'REMOTE_SCRIPT'
set -euo pipefail
cd "$REMOTE"

# Clear the whole directory: never deploy a stale artifact.
rm -rf dist

./scripts/build-release.sh
test -f dist/svdp-servware.zip

# macOS privacy protection (TCC) blocks SSH sessions from ~/Desktop unless the
# remote-login daemon has Full Disk Access, and that can lapse across a reboot.
# Landing the files somewhere reachable matters more than landing them on the
# Desktop, so fall back rather than failing a build that already succeeded.
DEST="$HOME/Desktop"
if ! ls "$DEST" >/dev/null 2>&1; then
  DEST="$HOME/svdp-install"
  mkdir -p "$DEST"
  echo "!! ~/Desktop is not reachable over SSH (macOS Full Disk Access)."
  echo "!! Putting the files in $DEST instead."
  echo "!! To use the Desktop: System Settings > Privacy & Security >"
  echo "!! Full Disk Access, and enable it for sshd (or Remote Login)."
fi

# Retire artifacts from earlier naming schemes. This project has twice been
# bitten by a stale artifact sitting next to a fresh one; the destination needs
# clearing, not just the build directory. (Unmatched globs are harmless here:
# this half runs under bash, where rm -f simply gets the literal pattern.)
rm -f "$DEST"/svdp-skill-*.zip "$DEST"/svdp-skills.zip "$DEST"/svdp-servware.mcpb

cp dist/svdp-servware.zip "$DEST"/
xattr -c "$DEST"/svdp-servware.zip 2>/dev/null || true

echo "==> delivered to $DEST at $(date '+%H:%M:%S')"
echo "    $(basename "$DEST"/svdp-servware.zip)  $(stat -f %z "$DEST"/svdp-servware.zip) bytes"
unzip -Z1 "$DEST"/svdp-servware.zip | sed 's/^/      /' 
REMOTE_SCRIPT
