#!/usr/bin/env bash
# Publishes one report file to the `ci-report` branch, under the name given, so it can be read
# without opening the Actions UI:
#
#     publish.sh FILE NAME        e.g.  publish.sh report.md ci.md
#
# The branch holds one file per workflow (ci.md, pac-bench.md); every publish is a commit on top of
# it, so two workflows finishing at the same time do not overwrite each other (a rejected push is
# retried after fetching again). Needs GITHUB_TOKEN and GITHUB_REPOSITORY (or CI_REPORT_REMOTE for
# tests) and a token with contents: write. Never fails the build: a report that cannot be published
# is a warning, not a red run.
set -uo pipefail

file="$1"
name="$2"
branch="ci-report"
remote="${CI_REPORT_REMOTE:-https://x-access-token:${GITHUB_TOKEN:-}@github.com/${GITHUB_REPOSITORY:-}.git}"
work="$(mktemp -d)"

for attempt in 1 2 3 4 5; do
  rm -rf "$work/repo"
  if git ls-remote --exit-code --heads "$remote" "$branch" >/dev/null 2>&1; then
    git clone -q --depth 1 --branch "$branch" "$remote" "$work/repo" >/dev/null 2>&1 || { sleep "$attempt"; continue; }
  else
    git init -q -b "$branch" "$work/repo" && git -C "$work/repo" remote add origin "$remote"
  fi
  cp "$file" "$work/repo/$name"
  (
    cd "$work/repo" || exit 1
    git add -A
    git diff --cached --quiet && exit 0
    git -c user.name="hopswitch-ci" -c user.email="ci@hopswitch.invalid" \
      commit -q -m "ci report: $name for ${GITHUB_SHA:-local}"
    git push -q origin "HEAD:$branch"
  ) >/dev/null 2>&1 && { echo "published $name to $branch"; exit 0; }
  sleep $((attempt * 2))
done
echo "::warning::could not publish $name to the $branch branch"
exit 0
