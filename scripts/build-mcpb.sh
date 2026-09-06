#!/usr/bin/env bash
# Build a .mcpb bundle containing the natively-compiled MCP server.
# Must run on the target platform (the bundle carries a platform-specific binary).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT/dist}"
TOOLCHAIN="${SVDP_TOOLCHAIN:-}"        # e.g. SVDP_TOOLCHAIN=1.90.0
TARGET="${SVDP_TARGET:-}"              # e.g. x86_64-apple-darwin
# One bundle per platform: a .mcpb carries a platform-specific binary, so the
# manifest must declare the platform it is actually for.
PLATFORM="${SVDP_PLATFORM:-}"          # darwin | linux | win32
SUFFIX="${SVDP_SUFFIX:-}"              # appended to the bundle filename

cd "$ROOT"
echo "==> building release binary${TARGET:+ for $TARGET}"
cargo ${TOOLCHAIN:+"+$TOOLCHAIN"} build --release --bin mcp ${TARGET:+--target "$TARGET"}

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/bin"
BIN="target/${TARGET:+$TARGET/}release/mcp"
cp "$BIN" "$STAGE/bin/svdp-mcp"
chmod +x "$STAGE/bin/svdp-mcp"
cp mcpb/manifest.json "$STAGE/manifest.json"
if [ -n "$PLATFORM" ]; then
  python3 - "$STAGE/manifest.json" "$PLATFORM" <<'PYEOF'
import json, sys
path, platform = sys.argv[1], sys.argv[2]
m = json.load(open(path))
m.setdefault("compatibility", {})["platforms"] = [platform]
json.dump(m, open(path, "w"), indent=2)
PYEOF
fi

echo "==> validating manifest"
python3 - "$STAGE/manifest.json" "$ROOT/Cargo.toml" <<'PY'
import json, re, sys
m = json.load(open(sys.argv[1]))
# The manifest version is what Claude Desktop shows, and it silently drifted
# from the crate version once already. Keep them equal.
crate = re.search(r'^version = "([^"]+)"', open(sys.argv[2]).read(), re.M).group(1)
assert m["version"] == crate, f'manifest {m["version"]} != Cargo.toml {crate}'
for k in ("manifest_version", "name", "version", "description", "author", "server"):
    assert k in m, f"manifest missing required key: {k}"
s = m["server"]
assert s.get("type") in {"node", "python", "binary"}, f"bad server.type: {s.get('type')}"
assert "entry_point" in s and "mcp_config" in s, "server needs entry_point and mcp_config"
for name, f in m.get("user_config", {}).items():
    for k in ("type", "title", "description"):   # description IS required
        assert k in f, f"user_config.{name} missing required key: {k}"
print(f"    manifest ok: {m['name']} v{m['version']}, {len(m.get('user_config', {}))} config fields")
PY

mkdir -p "$OUT_DIR"
BUNDLE="$OUT_DIR/svdp-servware${SUFFIX}.mcpb"
rm -f "$BUNDLE"
( cd "$STAGE" && zip -qr "$BUNDLE" . )

# A bundle is only Gatekeeper-clean if it never carries the quarantine attribute.
command -v xattr >/dev/null && xattr -c "$BUNDLE" 2>/dev/null || true

echo "==> $BUNDLE  ($(du -h "$BUNDLE" | cut -f1))"
