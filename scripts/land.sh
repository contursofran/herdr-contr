#!/bin/sh
# Lands one pull request: update its branch, wait for CI, squash merge the exact head, update main.
# It installs nothing: the backend moved to the repository contr-core (#253).
#   scripts/land.sh <pr> [--after <pr>]
# --after: first wait until that pull request is not open any more (land a queue with one job each).
# Before the merge it runs `herdr-contr-orch pr check-boundary`: a file outside the brief's Boundary: part stops it.
# Run it from the main checkout. Each step that fails ends the script with status 1, and a line says which.
set -u
fail() { echo "land: $*" >&2; exit 1; }
[ $# -ge 1 ] || fail "usage: scripts/land.sh <pr> [--after <pr>]"
N=$1; shift
AFTER=""
while [ $# -gt 0 ]; do
  case "$1" in
    --after) AFTER=${2:-}; shift 2 || fail "--after needs a pull request number" ;;
    *) fail "unknown argument: $1" ;;
  esac
done
cd "$(dirname "$0")/.." || fail "no repository root"
state() { gh pr view "$1" --json state --jq .state; }

if [ -n "$AFTER" ]; then
  while [ "$(state "$AFTER")" = OPEN ]; do sleep 30; done
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

# Files outside the brief's boundary go back to the worker. The installed tool holds the boundaries that spawn
# kept, so it runs that one. Exit 1: files outside. Other errors (no tool, an older tool, gh): a warning only.
ORCH=${HERDR_CONTR_ORCH:-${HERDR_APP_HOME:-$HOME/.config/herdr-app}/herdr-contr/bin/herdr-contr-orch}
if [ -x "$ORCH" ]; then
  "$ORCH" pr check-boundary "$N"; rc=$?
  [ "$rc" = 1 ] && fail "pull request $N does not pass the boundary check; send the files above back to the worker"
  [ "$rc" = 0 ] || echo "land: boundary check skipped (exit $rc)" >&2
fi

gh pr merge "$N" --squash --match-head-commit "$SHA" || fail "the merge of $N failed"
[ "$(state "$N")" = MERGED ] || fail "pull request $N is not merged"
echo "land: merged $N at $SHA"

git switch -q main && git pull -q --ff-only || fail "cannot update main"
