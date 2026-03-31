#!/usr/bin/env bash
set -euo pipefail

# Run changeset version to bump npm package versions
bunx changeset version

# Read the new version from the runtime package (source of truth after changeset version)
VERSION=$(jq -r '.version' npm/runtime/package.json)

echo "Syncing version $VERSION to Cargo.toml and version.txt..."

# Sync to version.txt
echo "$VERSION" > version.txt

# Sync to Cargo.toml files
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
for cargo_toml in "$REPO_ROOT"/native/vtz/Cargo.toml \
                  "$REPO_ROOT"/native/vertz-compiler/Cargo.toml \
                  "$REPO_ROOT"/native/vertz-compiler-core/Cargo.toml; do
  if [ -f "$cargo_toml" ]; then
    sed -i "s/^version = \".*\"/version = \"$VERSION\"/" "$cargo_toml"
  fi
done

# Sync optionalDependencies versions in the selector package
jq --arg v "$VERSION" '.optionalDependencies |= with_entries(.value = $v)' \
  npm/runtime/package.json > npm/runtime/package.json.tmp \
  && mv npm/runtime/package.json.tmp npm/runtime/package.json

echo "✅ All versions synced to $VERSION"
