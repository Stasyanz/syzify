#!/usr/bin/env bash
# Prepare a release: bump the version everywhere, draft the changelog.
# Does NOT commit or tag — you review/polish CHANGELOG.md first (hybrid flow),
# then run the printed commit + tag commands.
#
#   scripts/release.sh <X.Y.Z>
set -euo pipefail

VERSION="${1:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: scripts/release.sh <X.Y.Z>  (e.g. 0.26.0)" >&2
  exit 1
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "error: working tree is not clean — commit or stash first." >&2
  exit 1
fi

echo "Bumping to v$VERSION …"

# 1. package.json + package-lock.json
npm version "$VERSION" --no-git-tag-version --allow-same-version >/dev/null

# 2. Cargo.toml (the single top-level package version) + sync Cargo.lock via check
perl -i -pe 'BEGIN{$done=0} if(!$done && /^version\s*=/){s/"[^"]*"/"'"$VERSION"'"/; $done=1}' \
  src-tauri/Cargo.toml
cargo check --quiet --manifest-path src-tauri/Cargo.toml >/dev/null

# 3. tauri.conf.json (first "version": "…" only)
perl -i -pe 'BEGIN{$d=0} if(!$d && /"version"\s*:/){s/("version"\s*:\s*)"[^"]*"/${1}"'"$VERSION"'"/; $d=1}' \
  src-tauri/tauri.conf.json

# 4. CHANGELOG — auto-draft with git-cliff if available, else stamp [Unreleased]
if command -v git-cliff >/dev/null 2>&1; then
  git-cliff --tag "v$VERSION" -o CHANGELOG.md
  echo "CHANGELOG.md regenerated with git-cliff."
else
  TODAY="$(date +%F)"
  perl -i -pe 'BEGIN{$d=0} if(!$d && /^## \[Unreleased\]/){s/^## \[Unreleased\].*$/## [Unreleased]\n\n## ['"$VERSION"'] - '"$TODAY"'/; $d=1}' \
    CHANGELOG.md
  echo "git-cliff not found — stamped [Unreleased] as [$VERSION]."
fi

# 5. THIRD-PARTY-NOTICES — regenerate so the attribution bundled into the
#    installers matches the dependency set this release actually ships.
npm run --silent gen:notices
echo "THIRD-PARTY-NOTICES.md regenerated."

cat <<EOF

Version bumped to v$VERSION in package.json, Cargo.{toml,lock}, tauri.conf.json.
Next:
  1. Review & polish CHANGELOG.md.
  2. git add -A && git commit -m "chore(release): v$VERSION"
  3. git tag v$VERSION
EOF
