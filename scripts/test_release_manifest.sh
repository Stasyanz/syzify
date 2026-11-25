#!/usr/bin/env bash
# Guard for the latest.json assembly in .github/workflows/release.yml.
#
# The release workflow runs from the TAG's commit, so a bug in the publish
# script costs a re-tag to fix. This executes the real script body against a
# synthetic artifact tree for all six build targets and checks both the happy
# path (10 signed platform entries) and the missing-signature abort.
set -euo pipefail
cd "$(dirname "$0")/.."
WF=.github/workflows/release.yml
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# Extract the publish run-block (drop the 10-space YAML indent), up to the
# gh-release part, which needs the network.
awk '/mapfile -t files/{go=1} /notes="/{go=0} go{sub(/^          /,""); print}' \
  "$WF" > "$WORK/publish.sh"
grep -q 'latest.json' "$WORK/publish.sh" \
  || { echo "FAIL: could not extract publish script from $WF" >&2; exit 1; }

# A signature as `tauri build` writes it: one line of base64 over minisign
# text whose trusted comment carries the version it was signed for (#322).
# `sig <name> [version]`; an empty version leaves the field out.
sig() {
  local version=${2-0.9.9} comment="timestamp:1700000000"$'\t'"file:$1"
  [ -n "$version" ] && comment+=$'\t'"version:$version"
  printf 'untrusted comment: signature from tauri secret key\nRUQ%s\ntrusted comment: %s\nAAAA\n' "$1" "$comment" \
    | base64 | tr -d '\n'
}

make_tree() {
  rm -rf "$WORK/sim"
  mkdir -p "$WORK/sim"
  cd "$WORK/sim"
  local t
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    mkdir -p "artifacts/syzify-$t/macos" "artifacts/syzify-$t/dmg"
    echo "app-$t" > "artifacts/syzify-$t/macos/Syzify.app.tar.gz"
    sig "sig-$t-apptar" > "artifacts/syzify-$t/macos/Syzify.app.tar.gz.sig"
    echo "dmg-$t" > "artifacts/syzify-$t/dmg/Syzify_0.9.9_${t%%-*}.dmg"
  done
  for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
    mkdir -p "artifacts/syzify-$t/appimage" "artifacts/syzify-$t/deb"
    echo "ai-$t" > "artifacts/syzify-$t/appimage/Syzify_0.9.9_${t%%-*}.AppImage"
    sig "sig-$t-appimage" > "artifacts/syzify-$t/appimage/Syzify_0.9.9_${t%%-*}.AppImage.sig"
    echo "deb-$t" > "artifacts/syzify-$t/deb/Syzify_0.9.9_${t%%-*}.deb"
    sig "sig-$t-deb" > "artifacts/syzify-$t/deb/Syzify_0.9.9_${t%%-*}.deb.sig"
  done
  for t in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do
    mkdir -p "artifacts/syzify-$t/msi" "artifacts/syzify-$t/nsis"
    echo "msi-$t" > "artifacts/syzify-$t/msi/Syzify_0.9.9_${t%%-*}_en-US.msi"
    sig "sig-$t-msi" > "artifacts/syzify-$t/msi/Syzify_0.9.9_${t%%-*}_en-US.msi.sig"
    echo "exe-$t" > "artifacts/syzify-$t/nsis/Syzify_0.9.9_${t%%-*}-setup.exe"
    sig "sig-$t-nsis" > "artifacts/syzify-$t/nsis/Syzify_0.9.9_${t%%-*}-setup.exe.sig"
  done
}

run() {
  GITHUB_REPOSITORY=Stasyanz/syzify GITHUB_REF_NAME=v0.9.9 \
    bash -euo pipefail "$WORK/publish.sh"
}

expect() { # <jq filter> <expected>
  local got
  got=$(jq -re "$1" aliases/latest.json)
  [ "$got" = "$2" ] || { echo "FAIL: $1 = '$got', expected '$2'" >&2; exit 1; }
}

# --- Happy path: full manifest, every entry pairing the right signature ---
make_tree
run
expect '.version' '0.9.9'
expect '.platforms | length' '10'
expect '.platforms | keys | join(",")' \
  'darwin-aarch64,darwin-x86_64,linux-aarch64,linux-aarch64-deb,linux-x86_64,linux-x86_64-deb,windows-aarch64,windows-aarch64-msi,windows-x86_64,windows-x86_64-msi'
# The updater picks its entry by install channel; each key must carry the
# signature of the SAME channel's bytes, and a tag-pinned URL.
expect '.platforms."darwin-aarch64".signature' "$(sig sig-aarch64-apple-darwin-apptar)"
expect '.platforms."darwin-aarch64".url' \
  'https://github.com/Stasyanz/syzify/releases/download/v0.9.9/Syzify-macos-arm64.app.tar.gz'
expect '.platforms."linux-x86_64".signature' "$(sig sig-x86_64-unknown-linux-gnu-appimage)"
expect '.platforms."linux-x86_64-deb".signature' "$(sig sig-x86_64-unknown-linux-gnu-deb)"
expect '.platforms."linux-x86_64-deb".url' \
  'https://github.com/Stasyanz/syzify/releases/download/v0.9.9/Syzify-linux-x64.deb'
expect '.platforms."windows-x86_64".signature' "$(sig sig-x86_64-pc-windows-msvc-nsis)"
expect '.platforms."windows-x86_64-msi".signature' "$(sig sig-x86_64-pc-windows-msvc-msi)"
expect '.platforms."windows-aarch64-msi".url' \
  'https://github.com/Stasyanz/syzify/releases/download/v0.9.9/Syzify-windows-arm64.msi'
# The macOS updater payloads exist under the URLs the manifest promises.
for a in Syzify-macos-arm64.app.tar.gz Syzify-macos-x64.app.tar.gz; do
  [ -f "aliases/$a" ] || { echo "FAIL: missing alias $a" >&2; exit 1; }
done

# --- A single missing signature must abort before any manifest exists ---
make_tree
rm artifacts/syzify-aarch64-unknown-linux-gnu/appimage/*.AppImage.sig
if run 2>/dev/null; then
  echo "FAIL: publish script accepted a missing signature" >&2
  exit 1
fi
[ ! -f aliases/latest.json ] \
  || { echo "FAIL: partial latest.json was written" >&2; exit 1; }

# --- A signature that names no version, or another, must abort (#322) ---
# requireSignedVersion makes every installed app refuse it.
for bad in "" "0.9.8"; do
  make_tree
  sig "sig-x86_64-pc-windows-msvc-msi" "$bad" \
    > artifacts/syzify-x86_64-pc-windows-msvc/msi/Syzify_0.9.9_x86_64_en-US.msi.sig
  if run 2>"$WORK/err"; then
    echo "FAIL: publish script accepted a signature for version '${bad}'" >&2
    exit 1
  fi
  grep -q "windows-x86_64-msi is signed for version '${bad}'" "$WORK/err" \
    || { echo "FAIL: no version error for '${bad}': $(cat "$WORK/err")" >&2; exit 1; }
  [ ! -f aliases/latest.json ] \
    || { echo "FAIL: latest.json written with a signature for '${bad}'" >&2; exit 1; }
done

# A version only inside another field (a file named so) is no version;
# a signature that is not base64 at all (GNU base64 refuses it, macOS
# decodes it to junk) names none either: both abort.
make_tree
sig "app-version:0.9.9.zip" "" \
  > artifacts/syzify-x86_64-pc-windows-msvc/msi/Syzify_0.9.9_x86_64_en-US.msi.sig
if run 2>/dev/null; then
  echo "FAIL: publish script took a version from the file name" >&2
  exit 1
fi
[ ! -f aliases/latest.json ] || { echo "FAIL: latest.json written (file-name version)" >&2; exit 1; }
make_tree
echo 'not base64 at all!!' \
  > artifacts/syzify-x86_64-pc-windows-msvc/msi/Syzify_0.9.9_x86_64_en-US.msi.sig
if run 2>/dev/null; then
  echo "FAIL: publish script accepted a signature that is not base64" >&2
  exit 1
fi
[ ! -f aliases/latest.json ] || { echo "FAIL: latest.json written (not base64)" >&2; exit 1; }

echo "release manifest script: OK"
