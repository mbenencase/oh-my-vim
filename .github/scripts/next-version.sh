#!/usr/bin/env bash
#
# Decide the next semantic version from the conventional commits merged since
# the last release tag. Prints the version on stdout, or the literal "none"
# when nothing in the range warrants a release -- that word is the signal the
# release workflow uses to stop before touching the repository.
#
# Usage: .github/scripts/next-version.sh
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# The workspace version is the single source of truth for the *current* version
# (all six crates inherit it via `version.workspace = true`), while the tag only
# fixes the *range* of commits to inspect. Reading the base from Cargo.toml
# means a hand-edited version is respected rather than silently overwritten.
current=$(sed -n '/^\[workspace\.package\]/,/^\[/{s/^version = "\([^"]*\)".*/\1/p;}' Cargo.toml | head -1)
if [[ ! "$current" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "next-version: could not read [workspace.package] version from Cargo.toml" >&2
    exit 1
fi

last_tag=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)
range="HEAD"
if [[ -n "$last_tag" ]]; then
    # A tag name that does not resolve would make `git log` fail inside the
    # process substitution below, where `set -e` cannot see it -- the classifier
    # would then read zero commits and report "nothing to release" on a range
    # that is full of features. Fail loudly instead.
    git rev-parse --verify --quiet "${last_tag}^{commit}" >/dev/null \
        || { echo "${0##*/}: tag '$last_tag' does not resolve to a commit" >&2; exit 1; }
    range="${last_tag}..HEAD"
fi

# --no-merges makes this work for both merge strategies: with squash merges the
# PR lands as one conventional commit, with merge commits the branch's own
# commits are what survive the filter. Either way the un-conventional
# "Merge pull request #N" subject never reaches the classifier.
bump=none
while IFS= read -r sha; do
    [[ -z "$sha" ]] && continue
    subject=$(git log -1 --format=%s "$sha")
    body=$(git log -1 --format=%b "$sha")

    if [[ "$subject" =~ ^[a-zA-Z]+(\([^\)]*\))?!: ]] \
        || grep -qE '^BREAKING[ -]CHANGE:' <<<"$body"; then
        bump=major
        break # major outranks everything, no point reading further
    elif [[ "$subject" =~ ^feat(\([^\)]*\))?: ]]; then
        [[ "$bump" != major ]] && bump=minor
    elif [[ "$subject" =~ ^(fix|perf|revert)(\([^\)]*\))?: ]]; then
        [[ "$bump" == none ]] && bump=patch
    fi
done < <(git log --format=%H --no-merges "$range")

if [[ "$bump" == none ]]; then
    echo none
    exit 0
fi

IFS=. read -r major minor patch <<<"$current"
case "$bump" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
esac

echo "${major}.${minor}.${patch}"
