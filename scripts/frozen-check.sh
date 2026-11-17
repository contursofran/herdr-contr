#!/bin/sh
# The backend moved to the repository contr-backend (#253). This check fails when a change since <base ref>
# touches a path of scripts/backend-paths.txt. The files below are allowed: the freeze and the last PR edit them.
#   scripts/frozen-check.sh <base ref>
# Exit 0: no backend path. Exit 1: backend paths, one on each line. Exit 2: bad arguments or no diff.
set -u
[ $# -eq 1 ] || { echo "usage: scripts/frozen-check.sh <base ref>" >&2; exit 2; }
cd "$(dirname "$0")/.." || exit 2
list=scripts/backend-paths.txt
[ -f "$list" ] || { echo "frozen-check: no $list" >&2; exit 2; }
files=$(git diff --name-only "$1...HEAD") || { echo "frozen-check: no diff against $1" >&2; exit 2; }

allowed() {
  case "$1" in
    README.md|AGENTS.md|.github/workflows/ci.yml|scripts/land.sh|xplat/README.md|kit/README.md) return 0 ;;
  esac
  return 1
}

# A list line that ends with `/` is a folder. Comments, blank lines and `==>` renames select nothing.
backend() {
  while IFS= read -r entry; do
    case "$entry" in
      ''|'#'*|*'==>'*) continue ;;
      */) case "$1" in "$entry"*) return 0 ;; esac ;;
      *) [ "$1" = "$entry" ] && return 0 ;;
    esac
  done < "$list"
  return 1
}

bad=""
while IFS= read -r f; do
  [ -n "$f" ] || continue
  allowed "$f" && continue
  backend "$f" && bad="$bad$f
"
done <<EOF
$files
EOF
if [ -n "$bad" ]; then
  echo "frozen-check: these files are in contr-backend now; change them there:" >&2
  printf '%s' "$bad"
  exit 1
fi
exit 0
