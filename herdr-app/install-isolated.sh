#!/bin/sh
# Installs the herdr-contr layout and mod into a test herdr-app home, never the real one.
# Usage: install-isolated.sh <herdr-app-home> [path/to/herdr-contr-feed]
#  - <herdr-app-home> must exist and must not be ~/.config/herdr-app. Run herdr-app with
#    HERDR_APP_HOME=<herdr-app-home> to use it.
#  - The sidecar binary, if given, is copied to <herdr-app-home>/herdr-contr/bin/herdr-contr-feed.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
[ $# -ge 1 ] || { echo "usage: $0 <herdr-app-home> [herdr-contr-feed]" >&2; exit 2; }
[ -d "$1" ] || { echo "STOP: $1 is not a folder" >&2; exit 1; }
dest="$(cd "$1" && pwd -P)"
real="$(cd "$HOME/.config/herdr-app" 2>/dev/null && pwd -P || true)"
case "$dest" in
  "$HOME/.config/herdr-app"|"$HOME/.config/herdr-app/"*) echo "STOP: $dest is the real herdr-app home" >&2; exit 1 ;;
esac
[ -n "$real" ] && [ "$dest" = "$real" ] && { echo "STOP: $dest is the real herdr-app home" >&2; exit 1; }
mkdir -p "$dest/layouts" "$dest/mods"
cp "$here"/layouts/*.json "$dest/layouts/"
mkdir -p "$dest/themes"
cp "$here"/themes/*.json "$dest/themes/"
# Keys: our sections replace the same sections in the home's keys.json; other sections stay.
python3 - "$here/keys.json" "$dest/keys.json" <<'PY'
import json, os, sys
ours = json.load(open(sys.argv[1]))
home = json.load(open(sys.argv[2])) if os.path.exists(sys.argv[2]) else {}
home.update(ours)
home.pop("mod:herdr-contr", None)  # an older build put the keys here too; it stopped every key
open(sys.argv[2], "w").write(json.dumps(home, indent=2) + "\n")
PY
# Stage the mod beside the old one and swap it in with two renames, so a running window that
# reloads on file changes never sees a half-copied folder.
rm -rf "$dest/mods/.herdr-contr-new" "$dest/mods/.herdr-contr-old"
cp -R "$here/mods/herdr-contr" "$dest/mods/.herdr-contr-new"
[ -d "$dest/mods/herdr-contr" ] && mv "$dest/mods/herdr-contr" "$dest/mods/.herdr-contr-old"
mv "$dest/mods/.herdr-contr-new" "$dest/mods/herdr-contr"
rm -rf "$dest/mods/.herdr-contr-old"
if [ $# -ge 2 ]; then
  mkdir -p "$dest/herdr-contr/bin"
  cp "$2" "$dest/herdr-contr/bin/herdr-contr-feed"
  chmod +x "$dest/herdr-contr/bin/herdr-contr-feed"
fi
echo "installed into $dest"
