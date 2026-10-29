#!/usr/bin/env bash
# Prints a Markdown section about the files of one GitHub release, and which expected ones are missing:
#
#     release_assets.sh RELEASE_NAME VERSION > extra.md
#
# Used by the release workflow's report (tools/ci/report.py --extra). Needs the `gh` CLI with GH_TOKEN
# and GITHUB_REPOSITORY. Never fails: a release that cannot be read is said so in the text.
set -uo pipefail

name="$1"
version="$2"
repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}"

required=(
  "hopswitch-$version-x86_64-unknown-linux-musl.tar.gz"
  "hopswitch_$version-1_amd64.deb"
  "hopswitch-$version-1.x86_64.rpm"
  "hopswitch-$version-aarch64-apple-darwin.tar.gz"
  "hopswitch-$version-x86_64-apple-darwin.tar.gz"
  "hopswitch-$version-x86_64-pc-windows-msvc.zip"
  "SHA256SUMS"
)
# The installer is built by a job that is allowed to fail.
optional=("hopswitch-$version-x86_64-pc-windows-msvc.msi")

echo "## Release \`$name\`"
echo
if ! listing=$(gh release view "$name" --repo "$repo" --json url,isPrerelease,assets \
  --jq '.url, .isPrerelease, (.assets[] | "\(.name)\t\(.size)")' 2>&1); then
  echo "The release could not be read: ${listing%%$'\n'*}"
  exit 0
fi

url=$(sed -n 1p <<<"$listing")
prerelease=$(sed -n 2p <<<"$listing")
files=$(sed -n '3,$p' <<<"$listing")
echo "- page: $url (pre-release: $prerelease)"
echo
echo "| file | size (bytes) |"
echo "| --- | --- |"
if [ -n "$files" ]; then
  while IFS=$'\t' read -r file size; do
    echo "| $file | $size |"
  done <<<"$files"
fi
echo

missing=()
for want in "${required[@]}"; do
  grep -q -F -x -- "$want" <(cut -f1 <<<"$files") || missing+=("$want")
done
if [ ${#missing[@]} -eq 0 ]; then
  echo "All expected files are there."
else
  echo "**Missing files:**"
  for want in "${missing[@]}"; do echo "- \`$want\`"; done
fi
for want in "${optional[@]}"; do
  grep -q -F -x -- "$want" <(cut -f1 <<<"$files") || echo "- optional, not built: \`$want\`"
done
exit 0
