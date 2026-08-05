#!/usr/bin/env bash
# Shared helpers for the examples asserts. Sourced, never executed.
#
# The asserts are jq simulations of the v0.1 primitive flows, so they
# run with no binary, no network, and no credentials. When a locally
# built binary IS present they do double duty: each simulation is
# cross-checked against what the real primitive emits on the same
# fixtures. That is the regression contract, in both directions.

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "  ok  $*"; }
note() { echo "  --  $*"; }

# Print the path to a locally built stave binary, or return 1.
# Assumes the caller has cd'd to the examples/ directory.
stave_bin() {
  local candidate
  for candidate in ../target/debug/stave ../target/release/stave; do
    if [[ -x "$candidate" ]]; then
      printf '%s' "$candidate"
      return 0
    fi
  done
  return 1
}

# Run the binary with the audit trail off. Repro output must never
# carry audit lines (see .claude/rules/tenant-data-hygiene.md).
run_stave() {
  local bin="$1"
  shift
  STAVE_AUDIT=off "$bin" "$@"
}

# True when `stave <verb> --help` advertises <needle>. A stale binary
# built before a recipe landed should skip that cross-check out loud
# rather than fail as though the fixtures were wrong.
stave_advertises() {
  local bin="$1" verb="$2" needle="$3"
  STAVE_AUDIT=off "$bin" "$verb" --help 2>&1 | grep -q -- "$needle"
}

# Key-order-independent JSONL comparison. jq preserves input key order;
# the Rust serializer does not promise the same order, so both sides are
# normalized with sorted keys before comparing.
normalize() { jq -S -c '.'; }
