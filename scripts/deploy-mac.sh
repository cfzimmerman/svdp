#!/usr/bin/env bash
# Build the .mcpb on the Mac and place it on the Desktop.
#
# Excludes are ANCHORED with a leading slash. An unanchored `servware/` also
# matches `src/servware/`, which once shipped a source tree missing the entire
# protocol layer and then deployed a stale bundle over the top of it.
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
ssh -o BatchMode=yes "$HOST" "
  set -euo pipefail
  cd '$REMOTE'
  rm -f dist/svdp-servware.mcpb          # never deploy a stale artifact
  SVDP_TOOLCHAIN=\${SVDP_TOOLCHAIN:-1.90.0} ./scripts/build-mcpb.sh
  test -f dist/svdp-servware.mcpb
  cp dist/svdp-servware.mcpb /Users/cory/Desktop/svdp-servware.mcpb
  xattr -c /Users/cory/Desktop/svdp-servware.mcpb
  echo \"==> Desktop: \$(stat -f %z /Users/cory/Desktop/svdp-servware.mcpb) bytes, built \$(date '+%H:%M:%S')\"
"
