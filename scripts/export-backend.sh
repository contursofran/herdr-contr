#!/bin/sh
# Makes the history of the repository contr-backend (#253) in a new folder:
# a fresh clone of `main` from `origin`, cut by `git filter-repo` to the paths
# in scripts/backend-paths.txt, then the checks before the first push. It
# writes only in <out folder>. It pushes nothing and creates no repository.
#
#   sh scripts/export-backend.sh [--no-test] [--rewrite-issues] <out folder>
#
#   --no-test         skip the cargo check
#   --rewrite-issues  change each `#123` after a space or `(` in the commit
#                     messages into `<owner>/<repo>#123`, the repository of
#                     `origin` (open question Q1 of the plan; off by default)
#
#   sh scripts/export-backend.sh --filter-message <owner>/<repo>
#                     write the message on stdin, with the change of
#                     --rewrite-issues, to stdout (for the tests)
#
# Exit: 0 done, 1 stopped (the message names the check), 2 bad arguments.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
paths="$here/backend-paths.txt"

die() { echo "export-backend: $*" >&2; exit 1; }
usage() { echo "usage: sh scripts/export-backend.sh [--no-test] [--rewrite-issues] <out folder>" >&2; exit 2; }

# A Python expression over `message` (bytes) that names the repository $1 in
# each issue number after a space or `(`.
rewrite_expr() {
  printf '%s' "__import__('re').sub(rb'(?<=[ (])#([0-9]+)', rb'$1#\\1', message)"
}

check_slug() {
  printf '%s' "$1" | grep -qE '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' || die "not an <owner>/<repo>: $1"
}

if [ "${1:-}" = "--filter-message" ]; then
  [ $# -eq 2 ] || usage
  check_slug "$2"
  exec python3 -c "import sys; message = sys.stdin.buffer.read(); sys.stdout.buffer.write($(rewrite_expr "$2"))"
fi

run_test=1
rewrite=0
out=
while [ $# -gt 0 ]; do
  case "$1" in
    --no-test) run_test=0 ;;
    --rewrite-issues) rewrite=1 ;;
    -*) usage ;;
    *) [ -z "$out" ] || usage; out=$1 ;;
  esac
  shift
done
[ -n "$out" ] || usage

# Step 1: the guards.
[ ! -e "$out" ] && [ ! -L "$out" ] || die "$out exists; give a folder that does not exist"
git filter-repo --version >/dev/null 2>&1 || die "git filter-repo is missing; install it: brew install git-filter-repo"

# Step 2: a fresh clone of main.
url=$(git -C "$here" remote get-url origin) || die "this checkout has no origin remote"
git clone --no-local --no-tags --single-branch --branch main "$url" "$out"
cd "$out"

# Step 3: the cut. filter-repo removes the origin remote.
if [ "$rewrite" = 1 ]; then
  slug=$(printf '%s' "$url" | sed -E 's#^(https://[^/]+/|git@[^:]+:)##; s#\.git$##')
  check_slug "$slug"
  git filter-repo --paths-from-file "$paths" --message-callback "return $(rewrite_expr "$slug")"
else
  git filter-repo --paths-from-file "$paths"
fi

# Step 4: the checks before the first push.
fail() { echo "FAIL: $1" >&2; exit 1; }
pass() { echo "ok: $1"; }

name="no mod"
{ [ ! -e herdr-app ] && [ ! -e LICENSE-APACHE ]; } || fail "$name"; pass "$name"

name="licence and notices"
{ [ -f LICENSE ] && [ -f NOTICE ] && [ -f THIRD_PARTY_NOTICES.md ]; } || fail "$name"; pass "$name"

name="one author"
n=$(git log --format='%an' | sort -u | wc -l | tr -d ' ')
[ "$n" = 1 ] || fail "$name ($n authors)"; pass "$name"

name="no AI attribution"
n=$(git log --format=%B | grep -ciE 'co-authored-by|generated with|claude\.com/claude-code' || true)
[ "$n" = 0 ] || fail "$name ($n lines)"; pass "$name"

# The fixtures use the made-up users me, you, u, a, x, test, o, someone and
# user (`/home/user2`).
name="no local paths"
found=$(git log -p | grep -E '^\+' | grep -oE '/(Users|home)/[a-z]+' \
  | grep -vE '^/(Users|home)/(me|you|u|a|x|test|o|someone|user)$' | sort -u || true)
[ -z "$found" ] || fail "$name: $(echo "$found" | tr '\n' ' ')"; pass "$name"

name="no secrets"
if command -v gitleaks >/dev/null 2>&1; then
  gitleaks git . >/dev/null 2>&1 || fail "$name (gitleaks git .)"
else
  n=$(git log -p | grep -cE 'ghp_[A-Za-z0-9]{30}|sk-ant-|BEGIN [A-Z ]*PRIVATE KEY' || true)
  [ "$n" = 0 ] || fail "$name ($n lines)"
fi
pass "$name"

name="no Apache text"
found=$(git grep -il 'apache license' -- xplat kit web scripts || true)
[ -z "$found" ] || fail "$name: $(echo "$found" | tr '\n' ' ')"; pass "$name"

name="it builds"
if [ "$run_test" = 1 ]; then
  cargo test --locked --no-default-features --manifest-path xplat/Cargo.toml \
    -p herdr-contr-feed -p herdr-core -p herdr-contr-orch || fail "$name"
  pass "$name"
else
  echo "skipped: $name (--no-test)"
fi

# Step 5.
echo "commits: $(git rev-list --count HEAD)"
echo "head: $(git rev-parse HEAD)"
