#!/usr/bin/env bash
# Assert 1: round-trip. list → filter(_kind=X) → emit(jsonl) returns the fixture unchanged.
#
# Simulates the v0.1 primitive flow in jq:
#   stave list <kind>                    ≈  cat fixtures/<kind>.jsonl
#   stave filter --where '_kind == "X"'  ≈  jq -c 'select(._kind == "X")'
#   stave emit --format jsonl            ≈  jq -c '.'
#
# Catches: missing fields, wrong types, accidental field renames,
# unparseable JSON, _kind mismatch with filename. When a built binary is
# present, also confirms `stave filter` agrees with the simulation.

set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=examples/asserts/_lib.sh
source asserts/_lib.sh

echo "== assert 01: round-trip =="

KINDS=(issue vulnerability_finding cloud_resource cloud_account)

bin=""
if bin="$(stave_bin)"; then
  note "cross-checking against $bin"
else
  note "no built binary under ../target; jq simulation only"
fi

for kind in "${KINDS[@]}"; do
  src="fixtures/${kind}.jsonl"
  [[ -f "$src" ]] || fail "$src missing"

  out="$(jq -c "select(._kind == \"$kind\")" "$src")"
  orig="$(jq -c '.' "$src")"
  [[ "$out" == "$orig" ]] || fail "$kind: filter(_kind=$kind) not byte-identical"
  pass "$kind round-trips through filter(_kind=$kind)"

  if [[ -n "$bin" ]]; then
    want="$(normalize < "$src")"
    got="$(run_stave "$bin" filter --where "_kind == \"$kind\"" < "$src" | normalize)"
    [[ "$got" == "$want" ]] || fail "$kind: stave filter disagrees with the jq simulation"
    pass "$kind matches stave filter"
  fi
done

# A predicate that selects nothing must emit nothing, not an error.
empty="$(jq -c 'select(._kind == "no_such_kind")' fixtures/issue.jsonl)"
[[ -z "$empty" ]] || fail "unmatched _kind should select no records"
pass "unmatched _kind selects nothing"

echo "assert 01 passed."
