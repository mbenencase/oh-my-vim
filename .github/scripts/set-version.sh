#!/usr/bin/env bash
#
# Write a new version into the workspace. Only [workspace.package] carries a
# version -- every crate inherits it with `version.workspace = true` and every
# internal dependency is declared `omv-*.workspace = true` (path-only, no
# version requirement), so this one line is the whole bump. Do not reintroduce
# `version = "x.y.z"` on the internal path deps: a `^0.1.0` requirement stops
# matching the moment the workspace crosses to 0.2.0 and the build breaks.
#
# Usage: .github/scripts/set-version.sh 1.2.3
set -euo pipefail

version=${1:?usage: set-version.sh <x.y.z>}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "set-version: '$version' is not x.y.z" >&2; exit 1; }

cd "$(git rev-parse --show-toplevel)"

sed -i -E '/^\[workspace\.package\]/,/^\[/ s/^version = "[^"]*"/version = "'"$version"'"/' Cargo.toml

written=$(sed -n '/^\[workspace\.package\]/,/^\[/{s/^version = "\([^"]*\)".*/\1/p;}' Cargo.toml | head -1)
[[ "$written" == "$version" ]] || { echo "set-version: Cargo.toml still reads '$written'" >&2; exit 1; }

# Keep Cargo.lock in step so the release commit is self-consistent and a
# `--locked` build of the tag resolves without wanting to rewrite the lockfile.
cargo update --workspace --offline 2>/dev/null || cargo update --workspace

echo "$version"
