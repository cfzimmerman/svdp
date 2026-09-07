#!/usr/bin/env bash
# Build a .mcpb bundle containing the compiled MCP server.
#
# The bundle carries a compiled binary, so it must be built for the platform it
# is for. On macOS, set SVDP_UNIVERSAL=1 to build both architectures and join
# them with `lipo` into one universal binary: volunteers then have a single Mac
# download and never have to work out whether their Mac is Apple silicon or
# Intel. It also removes any need for an Intel runner, which matters because
# GitHub retired macos-13 in December 2025 and drops x86_64 macOS entirely in
# 2027. See DECISIONS.md D30.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT/dist}"
TARGET="${SVDP_TARGET:-}"              # e.g. x86_64-apple-darwin
UNIVERSAL="${SVDP_UNIVERSAL:-}"        # macOS only: build a fat arm64+x86_64 binary
# One bundle per platform: a .mcpb carries a platform-specific binary, so the
# manifest must declare the platform it is actually for.
PLATFORM="${SVDP_PLATFORM:-}"          # darwin | linux | win32
SUFFIX="${SVDP_SUFFIX:-}"              # appended to the bundle filename

cd "$ROOT"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/bin"

if [ -n "$UNIVERSAL" ]; then
  ARM=aarch64-apple-darwin
  X86=x86_64-apple-darwin
  echo "==> building universal release binary ($ARM + $X86)"
  for t in "$ARM" "$X86"; do
    # No toolchain override: rust-toolchain.toml pins the version, and both
    # rustup and cargo honour it from the working directory. An override here
    # was a third place the Rust version lived, and it had gone stale.
    rustup target add "$t" >/dev/null
    cargo build --release --bin mcp --target "$t"
  done
  lipo -create -output "$STAGE/bin/svdp-mcp" \
    "target/$ARM/release/mcp" "target/$X86/release/mcp"
  # A bundle that silently carried one architecture would install fine and then
  # fail on half the volunteers' machines, so verify both are present.
  lipo -archs "$STAGE/bin/svdp-mcp" | tr ' ' '\n' | sort > "$STAGE/archs"
  printf 'arm64\nx86_64\n' | sort | diff -q - "$STAGE/archs" >/dev/null \
    || { echo "!! universal binary has: $(lipo -archs "$STAGE/bin/svdp-mcp")" >&2; exit 1; }
  echo "    archs: $(lipo -archs "$STAGE/bin/svdp-mcp")"
else
  echo "==> building release binary${TARGET:+ for $TARGET}"
  cargo build --release --bin mcp ${TARGET:+--target "$TARGET"}
  cp "target/${TARGET:+$TARGET/}release/mcp" "$STAGE/bin/svdp-mcp"
fi
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
