#!/usr/bin/env bash
# Guard for scripts/update-cask.sh. A wrong sha256 in the tap breaks
# `brew install` for everyone, so this runs the real script against a
# throwaway tap with a stubbed `gh` and checks the happy path and every abort.
set -euo pipefail
cd "$(dirname "$0")/.."
SCRIPT="$PWD/scripts/update-cask.sh"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

OLD_ARM=1111111111111111111111111111111111111111111111111111111111111111
OLD_INTEL=2222222222222222222222222222222222222222222222222222222222222222
NEW_ARM=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
NEW_INTEL=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb

# `gh release view` answers with whatever release.json holds.
mkdir -p "$WORK/bin"
printf '#!/usr/bin/env bash\ncat "%s/release.json"\n' "$WORK" > "$WORK/bin/gh"
chmod +x "$WORK/bin/gh"
export PATH="$WORK/bin:$PATH"

# The fields the script edits, laid out as in Stasyanz/homebrew-syzify.
make_tap() { # [cask body]
  rm -rf "$WORK/tap"
  mkdir -p "$WORK/tap/Casks"
  if [ $# -gt 0 ]; then
    printf '%s\n' "$1" > "$WORK/tap/Casks/syzify.rb"
  else
    printf '%s\n' \
      'cask "syzify" do' \
      '  arch arm: "aarch64", intel: "x64"' \
      '' \
      '  version "0.12.0"' \
      "  sha256 arm:   \"$OLD_ARM\"," \
      "         intel: \"$OLD_INTEL\"" \
      '' \
      '  url "https://github.com/Stasyanz/syzify/releases/download/v#{version}/Syzify_#{version}_#{arch}.dmg"' \
      'end' > "$WORK/tap/Casks/syzify.rb"
  fi
  git -C "$WORK/tap" init -q
  git -C "$WORK/tap" add -A
  git -C "$WORK/tap" -c user.name=test -c user.email=test@example.invalid commit -qm init
  cp "$WORK/tap/Casks/syzify.rb" "$WORK/before.rb"
}

release() { # <isDraft> <assets json array>
  printf '{"isDraft":%s,"assets":%s}\n' "$1" "$2" > "$WORK/release.json"
}

asset() { # <name> <digest>
  printf '{"name":"%s","digest":"%s"}' "$1" "$2"
}

FULL="[$(asset Syzify-macos-arm64.dmg "sha256:$OLD_ARM"),$(asset Syzify_0.13.0_aarch64.dmg "sha256:$NEW_ARM"),$(asset Syzify_0.13.0_x64.dmg "sha256:$NEW_INTEL"),$(asset latest.json "sha256:$OLD_INTEL")]"

fail() { echo "FAIL: $*" >&2; exit 1; }

expect_abort() { # <label> <args...>
  local label=$1; shift
  if bash "$SCRIPT" "$@" >/dev/null 2>&1; then
    fail "$label: the script succeeded"
  fi
  cmp -s "$WORK/before.rb" "$WORK/tap/Casks/syzify.rb" || fail "$label: the cask was modified"
  [ ! -e "$WORK/tap/Casks/syzify.rb.tmp" ] || fail "$label: a temp file was left behind"
}

# --- Happy path: version and both digests, nothing else ---
make_tap
release false "$FULL"
bash "$SCRIPT" 0.13.0 "$WORK/tap" >/dev/null
CASK="$WORK/tap/Casks/syzify.rb"
grep -qx '  version "0.13.0"' "$CASK" || fail "version not bumped"
grep -qx "  sha256 arm:   \"$NEW_ARM\"," "$CASK" || fail "arm digest not written"
grep -qx "         intel: \"$NEW_INTEL\"" "$CASK" || fail "intel digest not written"
grep -qx '  arch arm: "aarch64", intel: "x64"' "$CASK" || fail "arch line changed"
[ "$(git -C "$WORK/tap" diff --numstat | cut -f1)" = 3 ] || fail "expected exactly 3 changed lines"
[ ! -e "$CASK.tmp" ] || fail "temp file left behind"

# SYZIFY_TAP stands in for the tap argument; a re-run is a no-op.
git -C "$WORK/tap" -c user.name=test -c user.email=test@example.invalid commit -qam bump
SYZIFY_TAP="$WORK/tap" bash "$SCRIPT" 0.13.0 >/dev/null
[ -z "$(git -C "$WORK/tap" status --porcelain)" ] || fail "re-run changed the cask"

# --- Aborts: the cask stays byte-for-byte as it was ---
make_tap
release false "$FULL"
expect_abort "bad version" 0.13 "$WORK/tap"
expect_abort "no tap" 0.13.0 "$WORK/nowhere"

release true "$FULL"
expect_abort "draft release" 0.13.0 "$WORK/tap"

release false "[$(asset Syzify_0.13.0_aarch64.dmg "sha256:$NEW_ARM")]"
expect_abort "missing x64 dmg" 0.13.0 "$WORK/tap"

release false "[$(asset Syzify_0.13.0_aarch64.dmg "sha256:$NEW_ARM"),$(asset Syzify_0.13.0_x64.dmg "")]"
expect_abort "empty digest" 0.13.0 "$WORK/tap"

release false "[$(asset Syzify_0.13.0_aarch64.dmg "md5:abc"),$(asset Syzify_0.13.0_x64.dmg "sha256:$NEW_INTEL")]"
expect_abort "non-sha256 digest" 0.13.0 "$WORK/tap"

release false "$FULL"
echo '# local edit' >> "$WORK/tap/Casks/syzify.rb"
cp "$WORK/tap/Casks/syzify.rb" "$WORK/before.rb"
expect_abort "dirty cask" 0.13.0 "$WORK/tap"

make_tap "$(printf '%s\n' 'cask "syzify" do' '  version "0.12.0"' "  sha256 \"$OLD_ARM\"" 'end')"
expect_abort "changed layout" 0.13.0 "$WORK/tap"

echo "update-cask script: OK"
