#!/usr/bin/env bash
# Literal membership test: are the scope names the operation registry
# declares actually present in an account's granted scope list?
#
#   stave --profile <p> auth scopes --from-directory | scripts/scope-membership.sh
#
# Why this exists. `auth can-i` and `auth plan --check` run through
# `scope_granted`, which treats `read:all` as satisfying any `read:*`
# requirement. Under a credential holding `read:all` those verdicts are
# vacuous: they pass whether or not the tenant recognises the name. This
# applies no implication at all, so a pass means the literal string is in
# the granted set. See aae-orc-h9uo.
#
# Prints one verdict per declared scope and, for absent ones, the granted
# names sharing the same resource word. It never prints the full granted
# list, which on a development credential is 79 entries of privilege
# posture nobody needs to read.
#
# Exit codes: 0 all declared scopes present, 1 one or more absent,
# 2 usage or input error.

set -euo pipefail

for tool in jq; do
    command -v "$tool" >/dev/null || { echo "need $tool" >&2; exit 2; }
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
registry="$repo_root/crates/stave-api/src"

# Derive the declared set from the registry rather than restating it, so
# this cannot drift from `required_scopes` the way a hardcoded copy would.
if [[ ! -d "$registry" ]]; then
    echo "cannot find the operation registry at $registry" >&2
    exit 2
fi
declared="$(grep -rhoE '"(read|write|admin|create|update|delete):[a-z_]+"' "$registry" \
    | tr -d '"' | sort -u)"
[[ -n "$declared" ]] || { echo "no declared scopes found in $registry" >&2; exit 2; }

input="$(cat)"
granted="$(jq -er '.scopes // empty | .[]' <<<"$input" 2>/dev/null || true)"
if [[ -z "$granted" ]]; then
    echo "no .scopes array on stdin; expected the output of 'auth scopes'" >&2
    exit 2
fi

count="$(wc -l <<<"$granted" | tr -d ' ')"
read_all="no"
grep -qxF 'read:all' <<<"$granted" && read_all="YES"
printf 'granted set size: %s\nread:all literally present: %s\n\n' "$count" "$read_all"

absent=()
while IFS= read -r scope; do
    if grep -qxF "$scope" <<<"$granted"; then
        printf '  OK      %s\n' "$scope"
    else
        printf '  ABSENT  %s\n' "$scope"
        absent+=("$scope")
    fi
done <<<"$declared"

if (( ${#absent[@]} > 0 )); then
    printf '\n'
    for scope in "${absent[@]}"; do
        # The resource stem, singularised. "read:users" has to match
        # "read:user_accounts", which is the whole point of this branch
        # and which the plural form misses, so the trailing s comes off.
        word="${scope#*:}"
        word="${word%%_*}"
        word="${word%s}"
        near="$(grep -F "$word" <<<"$granted" | sort | paste -sd', ' - || true)"
        printf '  near-matches for %s: %s\n' "$scope" "${near:-(none)}"
    done
    exit 1
fi
