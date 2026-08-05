#!/usr/bin/env bash
# Assert 2: cross-kind enrich. cloud_resource.subscriptionExternalId
# joins to cloud_account.externalId.
#
# Simulates `stave enrich --with account-context --accounts ...`: each
# cloud_resource gains `account: {id, name, externalId, cloudProvider,
# status}`, or null when no account in the auxiliary set owns that
# subscription. The orphan case is the point: an unowned subscription
# must be visible as data, not silently dropped.

set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=examples/asserts/_lib.sh
source asserts/_lib.sh

echo "== assert 02: cross-kind enrich (cloud_resource → cloud_account) =="

# jq has no first-class left join; do it record by record.
joined="$(while IFS= read -r line; do
  ext=$(printf '%s' "$line" | jq -r '.subscriptionExternalId // empty')
  acct=$(jq -c --arg id "$ext" \
    'select(.externalId == $id) | {id, name, externalId, cloudProvider, status}' \
    fixtures/cloud_account.jsonl)
  if [[ -n "$acct" ]]; then
    printf '%s' "$line" | jq -c --argjson acct "$acct" '. + {account: $acct}'
  else
    printf '%s' "$line" | jq -c '. + {account: null}'
  fi
done < fixtures/cloud_resource.jsonl)"

check_account() {
  local resource="$1" expect="$2" got
  got=$(printf '%s\n' "$joined" \
    | jq -r --arg n "$resource" 'select(.name == $n) | .account.name // "null"')
  [[ "$got" == "$expect" ]] || fail "$resource: expected account '$expect', got '$got'"
  pass "$resource → $expect"
}

check_account example-corp-audit-logs example-corp-prod
check_account example-corp-api-01 example-corp-prod
check_account example-corp-build-agent example-corp-sandbox
check_account example-corp-legacy-share null

# The join carries the provider through, so downstream predicates can
# group by cloud without a second lookup.
provider=$(printf '%s\n' "$joined" \
  | jq -r 'select(.name == "example-corp-build-agent") | .account.cloudProvider')
[[ "$provider" == "Azure" ]] || fail "expected Azure provider on the join, got '$provider'"
pass "join carries cloudProvider (Azure)"

# Records of other kinds pass through untouched.
untouched=$(jq -c 'has("account")' fixtures/issue.jsonl | sort -u)
[[ "$untouched" == "false" ]] || fail "issue records must not gain an account field"
pass "issue records pass through without an account field"

bin=""
if ! bin="$(stave_bin)"; then
  note "no built binary under ../target; jq simulation only"
elif ! stave_advertises "$bin" enrich account-context; then
  note "$bin predates the Wiz recipe set; jq simulation only"
else
  note "cross-checking against $bin"
  want="$(printf '%s\n' "$joined" | normalize)"
  got="$(run_stave "$bin" enrich --with account-context \
    --accounts fixtures/cloud_account.jsonl \
    < fixtures/cloud_resource.jsonl | normalize)"
  [[ "$got" == "$want" ]] || fail "stave enrich disagrees with the jq simulation"
  pass "matches stave enrich --with account-context"
fi

echo "assert 02 passed."
