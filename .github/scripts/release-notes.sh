#!/usr/bin/env bash
#
# Render the changelog section for a release: the conventional commits since
# the last tag, grouped by what they mean to a user. Housekeeping types
# (chore/ci/style/test/build) are deliberately dropped -- they are the bulk of
# the commits and none of them change the software anyone installs.
#
# Usage: .github/scripts/release-notes.sh <version> [previous-tag]
set -euo pipefail

version=${1:?usage: release-notes.sh <version> [previous-tag]}
cd "$(git rev-parse --show-toplevel)"

last_tag=${2:-$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)}
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

repo_url=""
if [[ -n "${GITHUB_REPOSITORY:-}" ]]; then
    repo_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY}"
fi

breaking=() features=() fixes=() perf=() other=()

# Rewrite "type(scope): summary" into "**scope**: summary" so the reader sees
# the affected crate rather than the type they are already grouped under.
entry() {
    local sha=$1 subject=$2 scope="" text=""
    scope=$(sed -nE 's/^[a-zA-Z]+\(([^)]*)\)!?:.*/\1/p' <<<"$subject")
    text=$(sed -E 's/^[a-zA-Z]+(\([^)]*\))?!?: *//' <<<"$subject")
    local line="- "
    [[ -n "$scope" ]] && line+="**${scope}**: "
    line+="$text"
    if [[ -n "$repo_url" ]]; then
        line+=" ([\`${sha:0:7}\`](${repo_url}/commit/${sha}))"
    else
        line+=" (\`${sha:0:7}\`)"
    fi
    printf '%s' "$line"
}

while IFS= read -r sha; do
    [[ -z "$sha" ]] && continue
    subject=$(git log -1 --format=%s "$sha")
    body=$(git log -1 --format=%b "$sha")
    line=$(entry "$sha" "$subject")

    if [[ "$subject" =~ ^[a-zA-Z]+(\([^\)]*\))?!: ]] || grep -qE '^BREAKING[ -]CHANGE:' <<<"$body"; then
        breaking+=("$line")
    elif [[ "$subject" =~ ^feat(\([^\)]*\))?: ]]; then
        features+=("$line")
    elif [[ "$subject" =~ ^(fix|revert)(\([^\)]*\))?: ]]; then
        fixes+=("$line")
    elif [[ "$subject" =~ ^perf(\([^\)]*\))?: ]]; then
        perf+=("$line")
    elif [[ "$subject" =~ ^(docs|refactor)(\([^\)]*\))?: ]]; then
        other+=("$line")
    fi
done < <(git log --format=%H --no-merges "$range")

section() {
    local title=$1; shift
    (($# == 0)) && return 0
    printf '### %s\n\n' "$title"
    printf '%s\n' "$@"
    printf '\n'
}

printf '## v%s\n\n' "$version"
section 'Breaking changes' "${breaking[@]}"
section 'Features' "${features[@]}"
section 'Bug fixes' "${fixes[@]}"
section 'Performance' "${perf[@]}"
section 'Other changes' "${other[@]}"

if [[ -n "$last_tag" && -n "$repo_url" ]]; then
    printf '**Full diff**: %s/compare/%s...v%s\n' "$repo_url" "$last_tag" "$version"
fi
