#!/bin/bash
# Build one app's Android APK on GitHub Actions instead of this PC, then sign it
# locally with the debug key (cheap) and put it in out/.
# Snapshots the current working tree (tracked + untracked, .gitignore respected)
# into an unsigned test commit WITHOUT touching the working tree, index or
# branches, pushes it to ci/<app>/<stamp>, waits for the run, downloads the APK,
# and deletes the temporary branch.
# usage: dev/ci-apk.sh <app> [version-label]     e.g. dev/ci-apk.sh hisab 0.1.0
# needs: gh (github-cli) logged in with repo + workflow access; push over SSH.
set -euo pipefail
app=$1; label=${2:-dev}
root=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel); cd "$root"
[ -d "apps/$app/src-tauri" ] || { echo "no app $app"; exit 1; }
command -v gh >/dev/null || { echo "gh not installed (pacman -S github-cli)"; exit 1; }
stamp=$(date +%Y%m%d-%H%M%S)-$$
branch="ci/$app/$stamp"
idx=$(mktemp); trap 'rm -f "$idx"' EXIT
GIT_INDEX_FILE=$idx git read-tree HEAD
GIT_INDEX_FILE=$idx git add -A
tree=$(GIT_INDEX_FILE=$idx git write-tree)
# Test commits: anonymous identity, unsigned (user rule).
commit=$(GIT_AUTHOR_NAME="no name" GIT_AUTHOR_EMAIL="noname@noemail.com" \
         GIT_COMMITTER_NAME="no name" GIT_COMMITTER_EMAIL="noname@noemail.com" \
         git commit-tree "$tree" -p HEAD -m "CI snapshot: $app APK ($stamp)")
echo "[ci-apk] pushing snapshot $commit to $branch" >&2
git push -q origin "$commit:refs/heads/$branch"
cleanup() { git push -q origin --delete "$branch" >/dev/null 2>&1 || true; }
trap 'rm -f "$idx"; cleanup' EXIT
# Wait for the run to appear, then follow it.
for i in $(seq 1 60); do
  run=$(gh run list --branch "$branch" --workflow android.yml --json databaseId -q '.[0].databaseId' 2>/dev/null || true)
  [ -n "$run" ] && break; sleep 5
done
[ -n "${run:-}" ] || { echo "[ci-apk] no workflow run started"; exit 1; }
echo "[ci-apk] run $run: $(gh run view "$run" --json url -q .url)" >&2
gh run watch "$run" --interval 30 --exit-status >/dev/null || { gh run view "$run" --log-failed | tail -60; exit 1; }
dl=$(mktemp -d); gh run download "$run" -n "apk-$app" -D "$dl"
apk=$(ls "$dl"/*.apk | head -1)
source /shared/linuxelf/config/env.sh >/dev/null 2>&1
export PATH=/shared/linuxelf/bin/android-studio-app/jbr/bin:$PATH
bt=$(ls -d "$ANDROID_HOME"/build-tools/36* | tail -1)
mkdir -p out; out="out/$app-$label-arm64.apk"; rm -f "$out" "$out.idsig"
"$bt/zipalign" -p -f 4 "$apk" "$dl/aligned.apk"
"$bt/apksigner" sign --ks "$ANDROID_USER_HOME/debug.keystore" --ks-pass pass:android --ks-key-alias androiddebugkey --out "$out" "$dl/aligned.apk" 2>/dev/null
"$bt/apksigner" verify "$out" 2>/dev/null
rm -rf "$dl"
echo "[ci-apk] signed APK: $root/$out"
