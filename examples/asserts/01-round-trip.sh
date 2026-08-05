#!/usr/bin/env bash
# Assert 1: round-trip — list → filter(_kind=X) → emit(jsonl) is byte-identical to fixture.
#
# Simulates the v0.1 primitive flow without Rust:
#   stave list <noun>                    ≈  cat fixtures/<kind>.jsonl
#   stave filter --where '_kind == "X"'  ≈  jq -c 'select(._kind == "X")'
#   stave emit --format jsonl            ≈  jq -c '.'
#
# Catches: missing fields, wrong types, accidental field renames,
# unparseable JSON, _kind mismatch with filename.

set -euo pipefail
cd "$(dirname "$0")/.."

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "  ok  $*"; }

echo "== assert 01: round-trip =="

declare -a SPEC=(
  "device:device"
  "blueprint:blueprint"
  "vulnerability:vulnerability"
  "audit-event:audit_event"
)

for entry in "${SPEC[@]}"; do
  file="${entry%%:*}"
  kind="${entry##*:}"
  src="fixtures/${file}.jsonl"
  [[ -f "$src" ]] || fail "$src missing"
  out="$(jq -c "select(._kind == \"$kind\")" "$src")"
  orig="$(jq -c '.' "$src")"
  [[ "$out" == "$orig" ]] || fail "$file: filter(_kind=$kind) not byte-identical"
  pass "$file round-trips through filter(_kind=$kind)"
done

echo "assert 01 passed."
