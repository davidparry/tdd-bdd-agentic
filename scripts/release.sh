#!/bin/sh
# Cut a bdd-cli release.
#
# Bumps the version in cli/Cargo.toml (patch by default, or the exact
# version given as the first argument), syncs Cargo.lock, folds
# everything in the working tree into the branch's single squashed
# commit, force-pushes it, and pushes the vX.Y.Z tag that triggers the
# release workflow.
#
# Usage:
#   scripts/release.sh          # 0.1.2 -> 0.1.3
#   scripts/release.sh 0.2.4    # explicit version (use when Cargo.toml is already there)
set -eu
cd "$(dirname "$0")/.."

CURRENT=$(sed -n 's/^version = "\(.*\)"$/\1/p' cli/Cargo.toml | head -1)
if [ $# -ge 1 ]; then
    NEXT="$1"
else
    NEXT=$(echo "$CURRENT" | awk -F. '{printf "%d.%d.%d", $1, $2, $3 + 1}')
fi
echo "bdd-cli $CURRENT -> $NEXT"

echo "running the test suite first..."
(cd cli && cargo test --quiet)

perl -pi -e "s/^version = \"\Q$CURRENT\E\"$/version = \"$NEXT\"/" cli/Cargo.toml
(cd cli && cargo update --workspace --quiet)

BRANCH=$(git rev-parse --abbrev-ref HEAD)
MESSAGE=$(git log -1 --format=%s)
echo "squashing everything into a single '$MESSAGE' commit on $BRANCH and force-pushing"
git add -A
git reset --quiet "$(git commit-tree "$(git write-tree)" -m "$MESSAGE")"
git push --force origin "$BRANCH"

git tag "v$NEXT"
git push origin "v$NEXT"
echo "tagged v$NEXT - the Release workflow is building it"
