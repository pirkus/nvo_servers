#!/bin/bash

# Release script for nvo_servers
# Usage: ./release.sh [patch|minor|major]

set -euo pipefail

# If a tag is present on HEAD (or provided via RELEASE_TAG/CIRCLE_TAG), use it directly
# `git tag --points-at HEAD` works for both lightweight and annotated tags
HEAD_TAG="${RELEASE_TAG:-${CIRCLE_TAG:-$(git tag --points-at HEAD | head -n 1)}}"

if [[ -n "$HEAD_TAG" ]]; then
    # Trim the leading 'v' if present
    NEW_VERSION="${HEAD_TAG#v}"
    echo "Using tag version: $NEW_VERSION"
else
    # Default to patch version when no tag is provided
    VERSION_TYPE=${1:-patch}

    # Get current version from Cargo.toml (first occurrence only)
    CURRENT_VERSION=$(grep -m1 "^version" Cargo.toml | sed 's/version = "\(.*\)"/\1/')
    echo "Current version: $CURRENT_VERSION"

    # Calculate new version
    IFS='.' read -ra VERSION_PARTS <<< "$CURRENT_VERSION"
    MAJOR=${VERSION_PARTS[0]}
    MINOR=${VERSION_PARTS[1]}
    PATCH=${VERSION_PARTS[2]}

    case $VERSION_TYPE in
        major)
            MAJOR=$((MAJOR + 1))
            MINOR=0
            PATCH=0
            ;;
        minor)
            MINOR=$((MINOR + 1))
            PATCH=0
            ;;
        patch)
            PATCH=$((PATCH + 1))
            ;;
        *)
            echo "Invalid version type. Use: patch, minor, or major"
            exit 1
            ;;
    esac

    NEW_VERSION="$MAJOR.$MINOR.$PATCH"
    echo "New version: $NEW_VERSION"
fi

# Update Cargo.toml (only the first version entry)
sed -i "0,/^version = \".*\"/s//version = \"$NEW_VERSION\"/" Cargo.toml

echo "Running tests..."
cargo test

# Commit and tag unless skipped or the tag already exists (useful for CI)
if [[ "${SKIP_COMMIT:-}" != "true" ]] && [[ "${CIRCLE_TAG:-}" == "" ]]; then
    if git rev-parse -q --verify "v$NEW_VERSION" >/dev/null; then
        echo "Tag v$NEW_VERSION already exists. Skipping commit and tag."
    else
        git add Cargo.toml
        git commit -m "Release v$NEW_VERSION"
        git tag -a "v$NEW_VERSION" -m "Release version $NEW_VERSION"
    fi
elif [[ "${CIRCLE_TAG:-}" != "" ]]; then
    echo "Running in CI with tag $CIRCLE_TAG. Skipping local commit and tag creation."
fi

echo "Release prepared! To publish, run:"
echo "  git push origin main --tags"
