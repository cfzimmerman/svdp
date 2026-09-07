#!/usr/bin/env bash
# Create double-clickable shortcuts that open Claude Desktop with the request
# already written out.
#
# This is the shortest path for a volunteer: click an icon, press Enter. No
# typing, no remembering what to say, no menu to find. See DECISIONS.md D18.
#
#   ./scripts/make-shortcut.sh                 # all three, onto the Desktop
#   ./scripts/make-shortcut.sh ~/Desktop start # just the getting-started one
set -euo pipefail

OUT_DIR="${1:-$HOME/Desktop}"
WHICH="${2:-all}"

start_label="Start Here - SVdP"
start_prompt="I have just installed the SVdP ServWare extension. Please tell me what it can do and whether it is set up properly."

deliveries_label="Record SVdP Deliveries"
deliveries_prompt="I did SVdP deliveries today and need to record them in ServWare. Please check that ServWare is working, then show me the families who are waiting so I can tell you which ones we delivered to."

lists_label="Get a List of SVdP Families"
lists_prompt="I need a list of SVdP families for a project. Please ask me what the project needs and what date range counts as recent, then pull the information into spreadsheets on my Desktop and help me work out the answer."

make_one() {
  local label="$1" prompt="$2"
  local encoded url target
  # Deep links require URL encoding; the limit is ~14k characters.
  encoded="$(python3 -c 'import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1],safe=""))' "$prompt")"
  url="claude://claude.ai/new?q=${encoded}"

  case "$(uname -s)" in
    Darwin)
      # A .webloc is the native macOS "double-click to open this URL" file.
      target="$OUT_DIR/$label.webloc"
      cat > "$target" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>URL</key>
	<string>${url}</string>
</dict>
</plist>
PLIST
      ;;
    Linux)
      target="$OUT_DIR/$(echo "$label" | tr ' ' '-').desktop"
      cat > "$target" <<DESKTOP
[Desktop Entry]
Type=Link
Name=${label}
URL=${url}
Icon=text-html
DESKTOP
      chmod +x "$target"
      ;;
    *)
      echo "unsupported platform: $(uname -s)" >&2
      exit 1
      ;;
  esac
  echo "created: $target"
}

mkdir -p "$OUT_DIR"
case "$WHICH" in
  start)      make_one "$start_label" "$start_prompt" ;;
  deliveries) make_one "$deliveries_label" "$deliveries_prompt" ;;
  lists)      make_one "$lists_label" "$lists_prompt" ;;
  all)
    make_one "$start_label" "$start_prompt"
    make_one "$deliveries_label" "$deliveries_prompt"
    make_one "$lists_label" "$lists_prompt"
    ;;
  *) echo "unknown shortcut: $WHICH (use start, deliveries, lists, or all)" >&2; exit 1 ;;
esac

echo
echo "Double-clicking one opens Claude Desktop with the request already written."
echo "The volunteer just presses Enter."
