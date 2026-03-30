#!/usr/bin/env bash
set -euo pipefail

# Bump version across all Cargo.toml and npm package.json files.
# Usage: ./scripts/bump-version.sh <new-version>

if [ -z "${1:-}" ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 0.2.0"
  exit 1
fi

VERSION="$1"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Bumping version to $VERSION..."

# 1. version.txt
echo "$VERSION" > "$REPO_ROOT/version.txt"

# 2. Cargo.toml packages
for cargo_toml in "$REPO_ROOT"/native/vtz/Cargo.toml \
                  "$REPO_ROOT"/native/vertz-compiler/Cargo.toml \
                  "$REPO_ROOT"/native/vertz-compiler-core/Cargo.toml; do
  if [ -f "$cargo_toml" ]; then
    sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" "$cargo_toml"
    echo "  Updated $cargo_toml"
  fi
done

# 3. npm package.json files
for pkg_json in "$REPO_ROOT"/npm/*/package.json; do
  if [ -f "$pkg_json" ]; then
    jq --arg v "$VERSION" '.version = $v' "$pkg_json" > "$pkg_json.tmp" && mv "$pkg_json.tmp" "$pkg_json"
    echo "  Updated $pkg_json"
  fi
done

# 4. Update optionalDependencies in the selector package
jq --arg v "$VERSION" '.optionalDependencies |= with_entries(.value = $v)' \
  "$REPO_ROOT/npm/runtime/package.json" > "$REPO_ROOT/npm/runtime/package.json.tmp" \
  && mv "$REPO_ROOT/npm/runtime/package.json.tmp" "$REPO_ROOT/npm/runtime/package.json"

echo ""
echo "✅ Version bumped to $VERSION"
echo "Next steps:"
echo "  git add -A && git commit -m \"chore: bump version to $VERSION\""
echo "  git tag v$VERSION"
echo "  git push origin main --tags"
