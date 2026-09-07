#!/usr/bin/env bash
# Package each skill as its own zip, into <out>/skills/.
#
# ONE ZIP PER SKILL, because Claude Desktop's skill upload accepts a **zip** --
# not a folder. These are collected into the single top-level release archive by
# `build-release.sh`, so a volunteer performs exactly one unzip and then adds
# each skill zip as it comes.
#
# Skills are not part of the .mcpb: the MCPB manifest has no `skills` field, so
# they install separately via Settings -> Capabilities -> Skills. See
# DECISIONS.md D26 and D27.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT/dist/skills}"
mkdir -p "$OUT_DIR"

shopt -s nullglob
found=0

for dir in "$ROOT"/skills/*/; do
  name="$(basename "$dir")"
  test -f "$dir/SKILL.md" || { echo "!! $name has no SKILL.md" >&2; exit 1; }

  # Validate the frontmatter before shipping. claude.ai rejects unknown keys
  # outright and truncates long descriptions, and finding that out during an
  # upload is a bad time to find it out.
  python3 - "$dir/SKILL.md" "$name" <<'PY'
import re, sys
path, dirname = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read()
m = re.match(r"^---\n(.*?)\n---\n", text, re.S)
assert m, f"{path}: no YAML frontmatter"
block = m.group(1)

keys = re.findall(r"^([A-Za-z_-]+):", block, re.M)
ALLOWED = {"name", "description", "license", "compatibility", "metadata", "allowed-tools"}
bad = [k for k in keys if k not in ALLOWED]
assert not bad, f"{path}: frontmatter keys not accepted by claude.ai: {bad}"

name = re.search(r"^name:\s*(.+)$", block, re.M)
desc = re.search(r"^description:\s*(.+)$", block, re.M)
assert name and desc, f"{path}: name and description are required"
name, desc = name.group(1).strip(), desc.group(1).strip()
assert name == dirname, f"{path}: name {name!r} must match directory {dirname!r}"
assert len(desc) < 200, f"{path}: description is {len(desc)} chars, keep it under 200"
print(f"    {dirname}: ok ({len(desc)} char description)")
PY

  # The archive root is the skill directory itself, i.e. what you get by
  # compressing the folder.
  zip_path="$OUT_DIR/$name.zip"
  rm -f "$zip_path"
  ( cd "$ROOT/skills" && zip -qr "$zip_path" "$name" -x '*.DS_Store' )
  echo "    -> $(basename "$zip_path") ($(du -h "$zip_path" | cut -f1))"
  found=$((found + 1))
done

test "$found" -gt 0 || { echo "!! no skills found under $ROOT/skills" >&2; exit 1; }
echo "==> $found skill zips in $OUT_DIR"
