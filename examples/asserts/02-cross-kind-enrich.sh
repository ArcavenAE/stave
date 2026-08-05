#!/usr/bin/env bash
# Assert 2: cross-kind enrich — device.blueprint_id → blueprint join.
#
# Simulates `stave enrich --with blueprint-context --blueprints ...`:
# each device gains `blueprint: {id, name}` or null when the parent
# blueprint is absent (orphan detection).

set -euo pipefail
cd "$(dirname "$0")/.."

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "  ok  $*"; }

echo "== assert 02: cross-kind enrich (device → blueprint) =="

joined="$(jq -c --slurpfile bps fixtures/blueprint.jsonl '
  . as $d
  | ($bps[] | select(.id == $d.blueprint_id)) as $bp // null
  | $d + {blueprint: (if $bp == null then null else {id: $bp.id, name: $bp.name} end)}
' fixtures/device.jsonl 2>/dev/null || true)"
# jq lacks first-class left-join; do it record by record instead.
joined="$(while IFS= read -r line; do
  bpid=$(printf '%s' "$line" | jq -r '.blueprint_id // empty')
  bp=$(jq -c --arg id "$bpid" 'select(.id == $id) | {id, name}' fixtures/blueprint.jsonl)
  if [[ -n "$bp" ]]; then
    printf '%s' "$line" | jq -c --argjson bp "$bp" '. + {blueprint: $bp}'
  else
    printf '%s' "$line" | jq -c '. + {blueprint: null}'
  fi
done < fixtures/device.jsonl)"

check() {
  local name="$1" expect="$2"
  local got
  got=$(printf '%s\n' "$joined" | jq -r --arg n "$name" 'select(.device_name == $n) | .blueprint.name // "null"')
  [[ "$got" == "$expect" ]] || fail "$name: expected blueprint '$expect', got '$got'"
  pass "$name → $expect"
}

check kestrel "Mac Fleet"
check osprey "Kiosk iPads"
check rocinante "Mac Fleet"
check skiff-ipad "null"

echo "assert 02 passed."
