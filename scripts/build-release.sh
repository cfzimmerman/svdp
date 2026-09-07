#!/usr/bin/env bash
# Assemble the one file a volunteer receives.
#
#   svdp-servware<suffix>.zip
#   └── svdp-servware/
#       ├── START-HERE.txt
#       ├── svdp-servware.mcpb      <- install as an extension
#       └── skills/
#           ├── pulling-svdp-data.zip        <- add each of these as a skill
#           └── recording-svdp-deliveries.zip
#
# One unzip, then two kinds of install. Claude Desktop takes a **zip** for a
# skill, not a folder, so the skills stay zipped inside the archive.
#
# The .mcpb carries a platform-specific binary, so this archive is per-platform
# too; the skills inside are identical everywhere. Inside the folder the bundle
# is named without a platform suffix, so the instructions read the same on every
# machine.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT/dist}"
SUFFIX="${SVDP_SUFFIX:-}"
mkdir -p "$OUT_DIR"

"$ROOT/scripts/build-mcpb.sh" "$OUT_DIR"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
TOP="$STAGE/svdp-servware"
mkdir -p "$TOP/skills"

cp "$OUT_DIR/svdp-servware${SUFFIX}.mcpb" "$TOP/svdp-servware.mcpb"
"$ROOT/scripts/build-skills.sh" "$TOP/skills"

# Instructions travel inside the archive: this audience does not read
# repositories or release notes. See DECISIONS.md D26.
cat > "$TOP/START-HERE.txt" <<'TXT'
SVdP ServWare — setting it up
=============================

You need a ServWare username and password. Your conference president issues
them. This cannot create one for you.

Do these in order. You only ever do this once.


1. INSTALL CLAUDE DESKTOP

   Get it from  claude.ai/download  and sign in.


2. ADD THE SVdP EXTENSION

   In Claude: Settings > Extensions.
   Drag the file  svdp-servware.mcpb  (in this folder) into that window.


3. TYPE IN YOUR SERVWARE USERNAME AND PASSWORD

   Still in Settings > Extensions, click "SVdP ServWare" and fill in the two
   boxes. They are stored by your own computer. They never appear in the
   conversation and nobody else can see them.


4. TURN IT ON

   A newly added extension starts switched OFF. Make sure the switch next to
   it is on, or nothing will happen.


5. ADD THE SKILLS  (optional, but recommended)

   These make the conversation more guided.

   In Claude: Settings > Capabilities > Skills.
   Add a skill, and choose one of the ZIP FILES in the "skills" folder here.
   Then do it again for the other one. Add them ONE AT A TIME.

   Leave these zip files zipped — Claude wants the zip, not a folder.

   Make sure each one is switched on.


THAT IS EVERYTHING.

Open a new chat and type:   what can you do?

It will tell you what it can do and whether anything is still missing.


Then just say what you want, in your own words. For example:

   I did deliveries today and need to record them.

   I need a list of families with children under 13 for Adopt-a-Family.
TXT

ARCHIVE="$OUT_DIR/svdp-servware${SUFFIX}.zip"
rm -f "$ARCHIVE"
( cd "$STAGE" && zip -qr "$ARCHIVE" svdp-servware -x '*.DS_Store' )
echo "==> $ARCHIVE ($(du -h "$ARCHIVE" | cut -f1))"
unzip -Z1 "$ARCHIVE" | sed 's/^/      /'
