#!/bin/sh
# Lands one pull request: update its branch, wait for CI, squash merge the exact head, install merged main.
#   scripts/land.sh <pr> [--after <pr>] [--shot <file>]
# --after: first wait until that pull request is not open any more (land a queue with one job each).
# --shot:  after the install, write a picture of the Herdr window to that file.
# Run it from the main checkout. Each step that fails ends the script with status 1, and a line says which.
set -u
fail() { echo "land: $*" >&2; exit 1; }
[ $# -ge 1 ] || fail "usage: scripts/land.sh <pr> [--after <pr>] [--shot <file>]"
N=$1; shift
AFTER=""; SHOT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --after) AFTER=${2:-}; shift 2 || fail "--after needs a pull request number" ;;
    --shot) SHOT=${2:-}; shift 2 || fail "--shot needs a file" ;;
    *) fail "unknown argument: $1" ;;
  esac
done
cd "$(dirname "$0")/.." || fail "no repository root"
state() { gh pr view "$1" --json state --jq .state; }

if [ -n "$AFTER" ]; then
  while [ "$(state "$AFTER")" = OPEN ]; do sleep 30; done
  # The install of the pull request before this one must end first.
  sleep 150
fi
[ "$(state "$N")" = OPEN ] || fail "pull request $N is not open"

# A branch that is behind main gets main merged in; a conflict needs a merge by hand.
out=$(gh pr update-branch "$N" 2>&1) || case "$out" in
  *"already up-to-date"*|*"already up to date"*) ;;
  *) fail "cannot update the branch of $N: $out" ;;
esac
sleep 20
SHA=$(gh pr view "$N" --json headRefOid --jq .headRefOid) || fail "cannot read the head of $N"

# Wait for the checks of that head. `gh pr checks` fails while one is pending or has failed.
while gh pr checks "$N" 2>&1 | grep -qE "pending|no checks reported"; do sleep 20; done
gh pr checks "$N" >/dev/null 2>&1 || { gh pr checks "$N"; fail "the checks of $N did not pass"; }

gh pr merge "$N" --squash --match-head-commit "$SHA" || fail "the merge of $N failed"
[ "$(state "$N")" = MERGED ] || fail "pull request $N is not merged"
echo "land: merged $N at $SHA"

git switch -q main && git pull -q --ff-only || fail "cannot update main"
(cd xplat && cargo build -q --release -p herdr-contr-feed) || fail "the helper did not build"
H=$PWD/xplat/target/release/herdr-contr-feed
"$H" install --dry-run --helper "$H" >/dev/null || fail "the install dry run failed"
"$H" install --helper "$H" || fail "the install failed"
echo "land: installed main after $N"

if [ -n "$SHOT" ]; then
  # The app loads the new files by itself; give it time before the picture.
  sleep 60
  herdr-app shot "$SHOT" || fail "no picture"
fi
