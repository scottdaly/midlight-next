#!/bin/bash
# Version bump script for Midlight Next
# Usage: ./scripts/version.sh <version>
# Example: ./scripts/version.sh 0.2.0

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.2.0"
    exit 1
fi

VERSION=$1

# Validate version format (semver)
if ! [[ $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    echo "Error: Invalid version format. Use semver (e.g., 0.2.0 or 0.2.0-beta.1)"
    exit 1
fi

echo "Bumping version to $VERSION..."

# Get the script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Update apps/desktop/package.json
DESKTOP_PKG="$PROJECT_ROOT/apps/desktop/package.json"
if [ -f "$DESKTOP_PKG" ]; then
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$DESKTOP_PKG"
    else
        sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$DESKTOP_PKG"
    fi
    echo "  Updated $DESKTOP_PKG"
fi

# Update apps/desktop/src-tauri/Cargo.toml
CARGO_TOML="$PROJECT_ROOT/apps/desktop/src-tauri/Cargo.toml"
if [ -f "$CARGO_TOML" ]; then
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i '' "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" "$CARGO_TOML"
    else
        sed -i "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" "$CARGO_TOML"
    fi
    echo "  Updated $CARGO_TOML"
fi

# Update apps/desktop/src-tauri/tauri.conf.json
TAURI_CONF="$PROJECT_ROOT/apps/desktop/src-tauri/tauri.conf.json"
if [ -f "$TAURI_CONF" ]; then
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$TAURI_CONF"
    else
        sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$TAURI_CONF"
    fi
    echo "  Updated $TAURI_CONF"
fi

echo ""
echo "Version updated to $VERSION"
echo ""
echo "Next steps:"
echo "  1. Review changes: git diff"
echo "  2. Commit: git commit -am \"chore: bump version to $VERSION\""
echo "  3. Tag: git tag v$VERSION"
echo "  4. Push: git push && git push --tags"
