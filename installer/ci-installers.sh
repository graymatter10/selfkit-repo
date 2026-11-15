#!/bin/bash
# Build and test the Windows installers on GitHub Actions from the CURRENT working
# tree, without touching the local index, branches or files, then download them.
# usage: installer/ci-installers.sh [installers|release] [out-dir]
#   installers (default): pushes ci/installers/<stamp> -> windows.yml only
#   release:              pushes ci/installers-release/<stamp> -> release.yml dry run
#                         (5 APKs + Windows + Linux, no GitHub Release is created)
#   out-dir: default /shared/selfkitextra/installers
# The snapshot commit is anonymous and unsigned (same as dev/ci-apk.sh); the branch is
# deleted afterwards. Docs: docs/release.md.
set -euo pipefail
mode=${1:-installers}; out=${2:-/shared/selfkitextra/installers}
root=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel); cd "$root"
case $mode in
  installers) prefix=ci/installers; workflows="windows.yml" ;;
  release) prefix=ci/installers-release; workflows="release.yml" ;;
  *) echo "usage: $0 [installers|release] [out-dir]"; exit 1 ;;
esac
stamp=$(date +%Y%m%d-%H%M%S)-$$
branch="$prefix/$stamp"
idx=$(mktemp)
GIT_INDEX_FILE=$idx git read-tree HEAD
GIT_INDEX_FILE=$idx git add -A
tree=$(GIT_INDEX_FILE=$idx git write-tree)
commit=$(GIT_AUTHOR_NAME="no name" GIT_AUTHOR_EMAIL="noname@noemail.com" \
         GIT_COMMITTER_NAME="no name" GIT_COMMITTER_EMAIL="noname@noemail.com" \
         git commit-tree "$tree" -p HEAD -m "CI snapshot: $mode ($stamp)")
rm -f "$idx"
echo "[ci-installers] pushing $commit to $branch" >&2
git push -q origin "$commit:refs/heads/$branch"
trap 'git push -q origin --delete "$branch" >/dev/null 2>&1 || true' EXIT
rc=0
for wf in $workflows; do
  run=""
  for i in $(seq 1 60); do
    run=$(gh run list --branch "$branch" --workflow "$wf" --json databaseId -q '.[0].databaseId' 2>/dev/null || true)
    [ -n "$run" ] && break; sleep 5
  done
  [ -n "$run" ] || { echo "[ci-installers] $wf did not start"; rc=1; continue; }
  echo "[ci-installers] $wf run $run: $(gh run view "$run" --json url -q .url)" >&2
  if gh run watch "$run" --interval 60 --exit-status >/dev/null; then
    echo "[ci-installers] $wf run $run passed" >&2
  else
    echo "[ci-installers] $wf run $run FAILED" >&2; gh run view "$run" --log-failed | tail -80; rc=1
  fi
  mkdir -p "$out"
  gh run download "$run" -D "$out/$stamp" 2>/dev/null || true
done
echo "[ci-installers] files in $out/$stamp:" >&2
find "$out/$stamp" -type f 2>/dev/null | sort >&2 || true
exit $rc
