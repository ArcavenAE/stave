#!/usr/bin/env bash
# walkthrough.sh: drive the run harness end to end against stub-stave and
# print the runlog it produces.
#
# This exists so the mining stage can see the shape of a runlog before
# any real run exists. Nothing here touches a tenant: argv[0] resolves to
# examples/runlog/stub-stave, and every record is synthetic.
#
#   examples/runlog/walkthrough.sh            # run, print, discard
#   examples/runlog/walkthrough.sh --write    # also refresh example-runlog.jsonl
#
# The coach blocks below are FIXTURES. In a real run each one is the
# verbatim output of the stave-safety-coach subagent, consulted before
# the invocation it names. Writing one by hand is forgery; see
# docs/design/runlog-harness.md, "The gate and its limits".

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
RUNLOG="$ROOT/scripts/runlog.sh"

WRITE=0
[[ "${1:-}" == "--write" ]] && WRITE=1

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# stub-stave stands in for the binary under the name the harness expects.
mkdir -p "$WORK/bin"
ln -s "$HERE/stub-stave" "$WORK/bin/stave"
export PATH="$WORK/bin:$PATH"

RUN="$WORK/run"
"$RUNLOG" init --runbook A1 --run-dir "$RUN" --session-id "example-A1-0001" >/dev/null
export STAVE_RUNLOG_DIR="$RUN"

# A coach block, as the subagent emits it.
coach() { # $1 verdict, $2 command, $3 reason, [$4 doubt, $5 to-resolve]
  printf 'VERDICT: %s\nCOMMAND: %s\nREASON: %s\n' "$1" "$2" "$3"
  if [[ "$1" == "HALT" ]]; then
    printf 'DOUBT: %s\nTO RESOLVE: %s\n' "$4" "$5"
  fi
}

say() { printf '\n== %s\n' "$1"; }

# --- step 1: pull the open issue population -------------------------
say "step 1"
"$RUNLOG" step --step 1 \
  --intent "get every open issue so the sweep has a population to age" \
  --criterion "one record per open issue, with severity and created date"

CMD=(stave list issue --limit 3)
CANON="$("$RUNLOG" canon -- "${CMD[@]}")"
coach CLEAR "$CANON" "Read-only list against a bounded limit; no filter verb, so no full-connection walk." \
  | "$RUNLOG" verdict --coach-file - -- "${CMD[@]}"
ISSUES="$("$RUNLOG" exec --out issues.jsonl -- "${CMD[@]}")"

# --- step 2: narrow to open, then do the ageing by hand -------------
say "step 2"
"$RUNLOG" step --step 2 \
  --intent "keep only the issues still open, then age each one against its severity SLA" \
  --criterion "each open issue carries an age in days and a past-SLA flag"

CMD=(stave filter --where 'status == "OPEN"')
CANON="$("$RUNLOG" canon -- "${CMD[@]}")"
coach CLEAR "$CANON" "Stream transform over an already-fetched file; opens no connection." \
  | "$RUNLOG" verdict --coach-file - -- "${CMD[@]}"
OPEN="$("$RUNLOG" exec --in "$ISSUES" --out open.jsonl -- "${CMD[@]}")"

"$RUNLOG" oob --tool jq \
  --purpose "derive a per-record age in days from a timestamp" \
  --text "jq -c '. + {age_days: ((now - (.createdAt|fromdate))/86400|floor)}'" \
  --survives-fix yes \
  --reason "the age is derived from a field stave already returns; no ticket removes this"

"$RUNLOG" oob --tool jq \
  --purpose "group and count on a composite key of severity and status" \
  --text "jq -s 'group_by([.severity,.status]) | map({k:.[0], n:length})'" \
  --survives-fix no --ticket gs23 \
  --reason "issuesGroupedByValue returns this from the server; the client-side group is document debt, not verb demand"

# --- step 3: the owner attribution the runbook exists for -----------
say "step 3"
"$RUNLOG" step --step 3 \
  --intent "name the owner to chase for each overdue issue" \
  --criterion "every overdue issue carries a contactable owner"

CMD=(stave list user --limit 50)
CANON="$("$RUNLOG" canon -- "${CMD[@]}")"
coach CLEAR "$CANON" "Read-only list; small bounded limit." \
  | "$RUNLOG" verdict --coach-file - -- "${CMD[@]}"
"$RUNLOG" exec --out users.jsonl -- "${CMD[@]}" >/dev/null

"$RUNLOG" dead-end --why \
  "the user list cannot be joined back to issues: Issue.assignee is unselected, so there is no key on the issue side" \
  -- "${CMD[@]}"

"$RUNLOG" friction \
  --what "the join key is redacted on both sides by the scrubber, so even a selected assignee id would not join through this harness" \
  --cost "the attribution half of the step is unreachable in this run"

"$RUNLOG" result --outcome unmet \
  --gap "owner attribution: Issue.assignee is unselected (field-surface audit, aae-orc-qijl). Recorded, not investigated."

# --- step 4: a halt, and the human ruling ---------------------------
say "step 4"
"$RUNLOG" step --step 4 \
  --intent "check whether any of these issues names a known-exploited CVE" \
  --criterion "each overdue issue is marked exploited or not"

CMD=(stave search vulnerability_finding CVE-2026-00000 --limit 5)
CANON="$("$RUNLOG" canon -- "${CMD[@]}")"
set +e
coach HALT "$CANON" "search filters client-side and walks the whole connection." \
  "search is an unconditional halt: the stated limit of 5 does not bound the read." \
  "a human decides whether this run may spend a full-connection walk on the tenant" \
  | "$RUNLOG" verdict --coach-file - -- "${CMD[@]}"
set -e

"$RUNLOG" resume --disposition skip \
  --ruling "skip the exploited-CVE check for the commissioning run; it is not needed to prove the correlation loop"

"$RUNLOG" result --outcome unmet --gap "skipped at the human's ruling after a coach halt"

# --- close ----------------------------------------------------------
say "reconcile"
"$RUNLOG" finish --note "worked example over stub-stave; no tenant was contacted"

say "runlog.jsonl"
jq -c . < "$RUN/runlog.jsonl"

if [[ "$WRITE" -eq 1 ]]; then
  # The run lives in a temp directory, so the two absolute paths in
  # run_start are machine noise. Everything else is verbatim.
  sed "s|$WORK/run|<run-dir>|g" "$RUN/runlog.jsonl" > "$HERE/example-runlog.jsonl"
  printf '\nwrote %s\n' "$HERE/example-runlog.jsonl"
fi
