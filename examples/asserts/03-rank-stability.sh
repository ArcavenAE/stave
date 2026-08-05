#!/usr/bin/env bash
# Assert 3: rank stability — severity sort of vulnerabilities is
# deterministic across runs, with explicit severity-int mapping and
# first_detection_date-desc tiebreak. Highest severity first.

set -euo pipefail
cd "$(dirname "$0")/.."

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "  ok  $*"; }

echo "== assert 03: rank stability =="

rank() {
  jq -r -s '
    def sev_rank: {"critical":4,"high":3,"medium":2,"low":1,"info":0}[.severity] // -1;
    sort_by([-(sev_rank), -(.first_detection_date | fromdateiso8601)])
    | .[] | .cve_id
  ' fixtures/vulnerability.jsonl
}

a="$(rank)"
b="$(rank)"
[[ "$a" == "$b" ]] || fail "rank not deterministic across runs"
pass "deterministic across two runs"

first="$(printf '%s\n' "$a" | head -1)"
crit="$(jq -r 'select(.severity == "critical") | .cve_id' fixtures/vulnerability.jsonl)"
[[ "$first" == "$crit" ]] || fail "highest severity must rank first: got $first, want $crit"
pass "critical CVE ranks first ($first)"

last="$(printf '%s\n' "$a" | tail -1)"
[[ "$last" == "CVE-2025-3333" ]] || fail "medium CVE must rank last: got $last"
pass "medium CVE ranks last ($last)"

echo "assert 03 passed."
