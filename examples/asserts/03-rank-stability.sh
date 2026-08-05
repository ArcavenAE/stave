#!/usr/bin/env bash
# Assert 3: rank stability. The severity ordering of vulnerability
# findings is deterministic across runs, with an explicit severity-int
# mapping and a firstDetectedAt-desc tiebreak. Highest severity first.
#
# The mapping mirrors `enrich::severity_rank` in the SDK (CRITICAL 4 down
# to INFORMATIONAL 0, unknown -1). Two findings share HIGH so the
# tiebreak is exercised rather than assumed. When a built binary is
# present, the severity-roll-up recipe feeding this ordering is
# cross-checked too.

set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=examples/asserts/_lib.sh
source asserts/_lib.sh

echo "== assert 03: rank stability =="

rank() {
  jq -r -s '
    def sev_rank:
      {"CRITICAL":4,"HIGH":3,"MEDIUM":2,"LOW":1,"INFORMATIONAL":0}[.vendorSeverity] // -1;
    sort_by([-(sev_rank), -(.firstDetectedAt | fromdateiso8601)])
    | .[] | .name
  ' fixtures/vulnerability_finding.jsonl
}

a="$(rank)"
b="$(rank)"
[[ "$a" == "$b" ]] || fail "rank not deterministic across runs"
pass "deterministic across two runs"

first="$(printf '%s\n' "$a" | head -1)"
crit="$(jq -r 'select(.vendorSeverity == "CRITICAL") | .name' \
  fixtures/vulnerability_finding.jsonl)"
[[ "$first" == "$crit" ]] || fail "highest severity must rank first: got $first, want $crit"
pass "CRITICAL finding ranks first ($first)"

last="$(printf '%s\n' "$a" | tail -1)"
low="$(jq -r 'select(.vendorSeverity == "LOW") | .name' \
  fixtures/vulnerability_finding.jsonl)"
[[ "$last" == "$low" ]] || fail "LOW finding must rank last: got $last, want $low"
pass "LOW finding ranks last ($last)"

# Equal severity falls back to firstDetectedAt descending: the newer of
# the two HIGH findings comes first.
highs="$(printf '%s\n' "$a" | sed -n '2,3p' | tr '\n' ' ')"
[[ "$highs" == "CVE-2026-10002 CVE-2026-10003 " ]] \
  || fail "HIGH tiebreak must be firstDetectedAt desc: got '$highs'"
pass "HIGH tiebreak orders newest first"

bin=""
if ! bin="$(stave_bin)"; then
  note "no built binary under ../target; jq simulation only"
elif ! stave_advertises "$bin" enrich account-context; then
  # severity-roll-up kept its name across the Wiz port but changed the
  # field it reads, so the recipe id alone does not identify a Wiz
  # build. account-context is the id that only exists on one side.
  note "$bin predates the Wiz recipe set; jq simulation only"
else
  note "cross-checking against $bin"
  # severity-roll-up is the recipe that normalizes the field this
  # ordering reads. Every finding must roll up to its own vendorSeverity.
  mismatch="$(run_stave "$bin" enrich --with severity-roll-up \
    < fixtures/vulnerability_finding.jsonl \
    | jq -r 'select(.severity_rollup != .vendorSeverity) | .name')"
  [[ -z "$mismatch" ]] || fail "severity_rollup disagrees with vendorSeverity for: $mismatch"
  pass "stave enrich --with severity-roll-up matches vendorSeverity for all findings"
fi

echo "assert 03 passed."
