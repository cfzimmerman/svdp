#!/usr/bin/env bash
# Create a double-clickable shortcut that opens Claude Desktop with the
# delivery-recording request already written out.
#
# This is the shortest path for a volunteer: click the icon, press Enter. No
# typing, no remembering what to say, no menu to find.
set -euo pipefail

OUT_DIR="${1:-$HOME/Desktop}"
LABEL="${SVDP_SHORTCUT_LABEL:-Record SVdP Deliveries}"

PROMPT="I did SVdP deliveries today and need to record them in ServWare. Please check that ServWare is working, then show me the families who are waiting so I can tell you which ones we delivered to."

# URL-encode the prompt (deep links require it; limit is ~14k characters).
ENCODED="$(python3 -c "
import urllib.parse, sys
print(urllib.parse.quote(sys.argv[1], safe=''))
" "$PROMPT")"
URL="claude://claude.ai/new?q=${ENCODED}"

mkdir -p "$OUT_DIR"
case "$(uname -s)" in
  Darwin)
    # A .webloc is the native macOS "double-click to open this URL" file.
    TARGET="$OUT_DIR/$LABEL.webloc"
    cat > "$TARGET" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>URL</key>
	<string>${URL}</string>
</dict>
</plist>
PLIST
    ;;
  Linux)
    TARGET="$OUT_DIR/$(echo "$LABEL" | tr ' ' '-').desktop"
    cat > "$TARGET" <<DESKTOP
[Desktop Entry]
Type=Link
Name=${LABEL}
URL=${URL}
Icon=text-html
DESKTOP
    chmod +x "$TARGET"
    ;;
  *)
    echo "unsupported platform: $(uname -s)" >&2
    exit 1
    ;;
esac

echo "created: $TARGET"
echo
echo "Double-clicking it opens Claude Desktop with the request already written."
echo "The volunteer just presses Enter."
