#!/usr/bin/env bash
# Build a .mcpb bundle containing the natively-compiled MCP server.
# Must run on the target platform (the bundle carries a platform-specific binary).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT/dist}"
TOOLCHAIN="${SVDP_TOOLCHAIN:-}"   # e.g. SVDP_TOOLCHAIN=1.90.0

cd "$ROOT"
echo "==> building release binary"
cargo ${TOOLCHAIN:+"+$TOOLCHAIN"} build --release --bin mcp

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/bin"
cp target/release/mcp "$STAGE/bin/svdp-mcp"
chmod +x "$STAGE/bin/svdp-mcp"
cp mcpb/manifest.json "$STAGE/manifest.json"

echo "==> validating manifest"
python3 - "$STAGE/manifest.json" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
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
BUNDLE="$OUT_DIR/svdp-servware.mcpb"
rm -f "$BUNDLE"
( cd "$STAGE" && zip -qr "$BUNDLE" . )

# A bundle is only Gatekeeper-clean if it never carries the quarantine attribute.
command -v xattr >/dev/null && xattr -c "$BUNDLE" 2>/dev/null || true

echo "==> $BUNDLE  ($(du -h "$BUNDLE" | cut -f1))"
