#!/usr/bin/env bash
# Point the Homebrew cask at a published release: the version and the two dmg
# sha256 values, taken from the digests GitHub computed when the release
# workflow uploaded the assets. Run it after the release is published.
# Does NOT commit — review the diff in the tap, then run the printed commands.
#
#   scripts/update-cask.sh <X.Y.Z> [tap-dir]
#
# tap-dir defaults to $SYZIFY_TAP, else ../homebrew-syzify next to this repo.
set -euo pipefail

VERSION="${1:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAP="${2:-${SYZIFY_TAP:-$ROOT/../homebrew-syzify}}"
REPO=Stasyanz/syzify

die() { echo "error: $*" >&2; exit 1; }

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: scripts/update-cask.sh <X.Y.Z> [tap-dir]  (e.g. 0.13.0)" >&2
  exit 1
fi
[ -f "$TAP/Casks/syzify.rb" ] \
  || die "no cask at $TAP/Casks/syzify.rb — pass the tap directory or set SYZIFY_TAP."
TAP="$(cd "$TAP" && pwd)"
CASK="$TAP/Casks/syzify.rb"
[ -z "$(git -C "$TAP" status --porcelain -- Casks/syzify.rb)" ] \
  || die "$CASK has uncommitted changes — commit or discard them first."

# The release workflow publishes a draft first; a draft's assets are not
# downloadable by brew, so the cask must not point at one.
RELEASE="$(gh release view "v$VERSION" --repo "$REPO" --json isDraft,assets)"
[ "$(jq -r .isDraft <<<"$RELEASE")" = false ] \
  || die "v$VERSION is still a draft — wait for the release workflow to publish it."

# The cask downloads Syzify_<version>_<arch>.dmg, so read exactly those assets.
digest() {
  jq -r --arg n "Syzify_${VERSION}_$1.dmg" \
    '.assets[] | select(.name == $n) | .digest // empty' <<<"$RELEASE" \
    | sed -n 's/^sha256:\([0-9a-f]\{64\}\)$/\1/p'
}
ARM="$(digest aarch64)"
INTEL="$(digest x64)"
[ -n "$ARM" ] || die "no sha256 digest for Syzify_${VERSION}_aarch64.dmg in v$VERSION."
[ -n "$INTEL" ] || die "no sha256 digest for Syzify_${VERSION}_x64.dmg in v$VERSION."

# All three fields or nothing: a cask whose layout changed is left untouched.
VERSION="$VERSION" ARM="$ARM" INTEL="$INTEL" perl -0777 -pe '
  my $n = 0;
  $n += s/^(\s*version\s+)"[^"]*"/$1"$ENV{VERSION}"/m;
  $n += s/(sha256\s+arm:\s*)"[0-9a-f]{64}"/$1"$ENV{ARM}"/;
  $n += s/(intel:\s*)"[0-9a-f]{64}"/$1"$ENV{INTEL}"/;
  $n == 3 or die "only $n of 3 cask fields matched — has the cask layout changed?\n";
' "$CASK" > "$CASK.tmp" || { rm -f "$CASK.tmp"; die "$CASK left unchanged."; }
mv "$CASK.tmp" "$CASK"

cat <<EOF
Cask updated to $VERSION:
  arm   $ARM
  intel $INTEL
Next:
  1. git -C "$TAP" diff
  2. git -C "$TAP" commit -am "syzify $VERSION" && git -C "$TAP" push
EOF
