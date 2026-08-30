#!/usr/bin/env bash
# runlog.sh: the run harness for the operator-runbook exercise.
#
# bd aae-orc-e4jo.9. Emits runlog.jsonl beside the audit trail, joinable
# to it on session_id and trace_id.
#
# The audit trail records one line per API call. It cannot record what a
# step was TRYING to achieve, what happened outside stave, whether the
# step met the runbook's own criterion, or which calls were dead ends.
# Those four are the mining signal, and this harness is where they are
# written down.
#
# Two properties hold BY CONSTRUCTION rather than by anyone remembering:
#
#   1. SCRUB. Tenant output reaches a durable artifact only through
#      `exec`, which pipes it through scripts/scrub.sh. There is no
#      bypass flag. If the scrubber refuses a shape, the output is
#      discarded and the step fails.
#
#   2. THE COACH GATE. `exec` refuses to run an invocation that has no
#      recorded CLEAR verdict whose COMMAND line matches, byte for byte,
#      the canonical text of the argv it is about to run. Verdicts are
#      single-use, and a HALT latches the whole run.
#
# The limits of both are documented, honestly and at length, in
# docs/design/runlog-harness.md. Read that before trusting either.
#
# Usage:
#   scripts/runlog.sh init --runbook A1 [--run-dir DIR] [--session-id ID]
#   scripts/runlog.sh canon -- stave list issue --limit 5
#   scripts/runlog.sh step --step 1 --intent "..." [--criterion "..."]
#   scripts/runlog.sh verdict --coach-file F -- stave list issue --limit 5
#   scripts/runlog.sh exec [--out NAME] [--in PATH] [--catalog] -- stave ...
#   scripts/runlog.sh oob --tool jq --purpose "..." --survives-fix no \
#                         --ticket qijl --text "jq -c ..."
#   scripts/runlog.sh dead-end --why "..." -- stave ...
#   scripts/runlog.sh result --outcome partial --gap "..."
#   scripts/runlog.sh friction --what "..." [--cost "..."]
#   scripts/runlog.sh resume --disposition skip --ruling "..."
#   scripts/runlog.sh finish [--outcome ...]
#   scripts/runlog.sh reconcile
#   scripts/runlog.sh selftest

set -euo pipefail

RUNLOG_SCHEMA_VERSION=1

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$HERE")"
SCRUB="$HERE/scrub.sh"

# Exit codes are part of this script's contract; the driving agent reads
# them to tell "the gate stopped me" from "the command failed".
EX_USAGE=2       # argv or usage error
EX_STATE=3       # run state wrong (no run, halted, missing input)
EX_GATE=4        # the coach gate refused: no matching CLEAR verdict
EX_SCRUB=5       # the scrubber refused the output shape; nothing written
EX_CMD=6         # the invocation itself failed

STAVE_BIN_NAME="${STAVE_BIN:-stave}"

die() { printf 'runlog.sh: %s\n' "$1" >&2; exit "${2:-$EX_USAGE}"; }

need() { command -v "$1" >/dev/null 2>&1 || die "missing dependency: $1"; }

# ---------------------------------------------------------------------
# Primitives
# ---------------------------------------------------------------------

utc_now() {
  perl -MTime::HiRes=time -e '
    my $t = time; my @g = gmtime($t);
    printf("%04d-%02d-%02dT%02d:%02d:%02d.%03dZ",
      $g[5]+1900, $g[4]+1, $g[3], $g[2], $g[1], $g[0], ($t-int($t))*1000);'
}

epoch_ms() { perl -MTime::HiRes=time -e 'printf("%d", time()*1000);'; }

sha256_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1
  fi
}

sha256_text() { printf '%s' "$1" | sha256_stdin; }

# Canonical single-line rendering of an argv. Deterministic, so the text
# the coach reviews and the argv that executes cannot drift apart: both
# are derived from the same array by this function.
canon_argv() {
  local out="" a
  for a in "$@"; do
    if [[ "$a" =~ ^[A-Za-z0-9_@%+=:,./-]+$ ]]; then
      out="$out $a"
    else
      out="$out '${a//\'/\'\\\'\'}'"
    fi
  done
  printf '%s\n' "${out# }"
}

# Free text written by the executor (intents, purposes, reasons) goes
# through the scrubber's pattern tier before it is recorded. That tier
# catches emails, GUIDs, ARNs, OCIDs, IPs, account ids, and the local
# literals. It does NOT catch a person's name, a bucket name, or a
# project slug: those have no shape. The rule stands that free text
# describes purpose in general terms; this is a backstop, not a licence.
scrub_text() {
  local s="$1"
  [[ -n "$s" ]] || { printf ''; return 0; }
  printf '%s' "$s" | "$SCRUB" --text
}

# ---------------------------------------------------------------------
# Run state
# ---------------------------------------------------------------------

RUN_DIR=""
RUN_ID=""
SESSION_ID=""
RUNBOOK=""
AUDIT_DIR=""
BINARY_PATH=""

resolve_run_dir() {
  local d="${1:-}"
  if [[ -n "$d" ]]; then RUN_DIR="$d"
  elif [[ -n "${STAVE_RUNLOG_DIR:-}" ]]; then RUN_DIR="$STAVE_RUNLOG_DIR"
  else die "no run directory: pass --run-dir or export STAVE_RUNLOG_DIR" "$EX_STATE"
  fi
}

load_run() {
  [[ -f "$RUN_DIR/run.env" ]] || die "no run at $RUN_DIR (run 'init' first)" "$EX_STATE"
  # shellcheck disable=SC1091
  . "$RUN_DIR/run.env"
  RUN_ID="$RUNLOG_RUN_ID"
  SESSION_ID="$RUNLOG_SESSION_ID"
  RUNBOOK="$RUNLOG_RUNBOOK"
  AUDIT_DIR="$RUNLOG_AUDIT_DIR"
  # The binary init resolved and skew-checked. Absent from runs whose init
  # predates this field; exec falls back to resolving STAVE_BIN_NAME then.
  BINARY_PATH="${RUNLOG_BINARY_PATH:-}"
}

require_not_halted() {
  [[ -f "$RUN_DIR/HALTED" ]] || return 0
  cat >&2 <<EOF
runlog.sh: this run is HALTED and will execute nothing further.

$(cat "$RUN_DIR/HALTED")

A halt stops the run, not just the step. It goes to the human, who
chooses skip, proceed with a modification, or stop. Record their ruling:

  scripts/runlog.sh resume --disposition skip|proceed-modified|stop \\
                           --ruling '<what the human decided>'
EOF
  exit "$EX_STATE"
}

next_seq() {
  local n=0
  [[ -f "$RUN_DIR/state/seq" ]] && n="$(cat "$RUN_DIR/state/seq")"
  n=$((n + 1))
  printf '%s' "$n" > "$RUN_DIR/state/seq"
  printf '%s' "$n"
}

current_step() {
  [[ -f "$RUN_DIR/state/step" ]] && cat "$RUN_DIR/state/step" || printf ''
}

# stdin: a JSON object of type-specific fields. $1: entry type.
append_entry() {
  local type="$1" seq ts step
  seq="$(next_seq)"; ts="$(utc_now)"; step="$(current_step)"
  jq -c \
    --argjson sv "$RUNLOG_SCHEMA_VERSION" \
    --arg rid "$RUN_ID" --arg sid "$SESSION_ID" --arg rb "$RUNBOOK" \
    --arg step "$step" --arg t "$type" --argjson seq "$seq" --arg ts "$ts" '
    {schema_version: $sv, run_id: $rid, session_id: $sid, runbook: $rb,
     step: (if $step == "" then null else ($step | tonumber) end),
     seq: $seq, ts: $ts, type: $t} + .
  ' >> "$RUN_DIR/runlog.jsonl"
}

# ---------------------------------------------------------------------
# init
# ---------------------------------------------------------------------

cmd_init() {
  local runbook="" dir="" sid="" audit="" skew_ok=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --runbook) runbook="${2:?}"; shift 2 ;;
      --run-dir) dir="${2:?}"; shift 2 ;;
      --session-id) sid="${2:?}"; shift 2 ;;
      --audit-dir) audit="${2:?}"; shift 2 ;;
      --allow-skew) skew_ok=1; shift ;;
      *) die "init: unknown argument: $1" ;;
    esac
  done
  [[ -n "$runbook" ]] || die "init: --runbook is required"
  [[ -n "$dir" ]] || dir="$REPO_ROOT/runs/${runbook}-$(date -u +%Y%m%dT%H%M%SZ)"
  [[ -e "$dir/run.env" ]] && die "init: a run already exists at $dir" "$EX_STATE"

  RUN_DIR="$dir"
  mkdir -p "$RUN_DIR/state" "$RUN_DIR/data" "$RUN_DIR/verdicts/consumed" "$RUN_DIR/tmp"
  [[ -n "$audit" ]] || audit="$RUN_DIR/audit"
  mkdir -p "$audit"

  # Belt and braces. The audit trail this run writes is stave's own
  # output and is tenant-identifying by construction; the scrubber never
  # sees it. A run directory must not be committable.
  printf '*\n' > "$RUN_DIR/.gitignore"

  RUN_ID="$(sha256_text "$dir-$(utc_now)-$$" | cut -c1-16)"
  [[ -n "$sid" ]] || sid="runlog-${runbook}-${RUN_ID}"
  SESSION_ID="$sid"
  RUNBOOK="$runbook"
  AUDIT_DIR="$audit"

  cat > "$RUN_DIR/run.env" <<EOF
RUNLOG_RUN_ID='$RUN_ID'
RUNLOG_SESSION_ID='$SESSION_ID'
RUNLOG_RUNBOOK='$RUNBOOK'
RUNLOG_AUDIT_DIR='$AUDIT_DIR'
EOF

  : > "$RUN_DIR/runlog.jsonl"
  printf '0' > "$RUN_DIR/state/seq"

  # Which binary, and does it match the tree we are recording?
  #
  # Commissioning run 1 (2026-08-07) ran /opt/homebrew/bin/stave at
  # alpha-20260806-120101-81df3bc against a tree at 98249c3, so the
  # fields the qijl widening had added were selected by the documents on
  # disk and absent from every record. The runlog said repo_commit
  # 98249c3, which made it an actively misleading record rather than an
  # incomplete one: every conclusion about what the tool can reach would
  # have been attributed to the wrong document set, and the judges'
  # surface.md describes the TREE's documents.
  #
  # So: record the binary's own identity, and refuse when the two
  # disagree. `stave --version` is on the coach's own fast path, which is
  # why it can run here without a verdict.
  local bin_path bin_ver bin_sha tree_sha bin_dirty=0 skew_reason="" skew_basis=""
  bin_path="$(command -v "$STAVE_BIN_NAME" 2>/dev/null || true)"
  [[ -n "$bin_path" ]] || die "init: no '$STAVE_BIN_NAME' on PATH" "$EX_STATE"
  # Absolutize. `command -v` returns a relative STAVE_BIN (./target/debug/
  # stave) verbatim; pinning that would re-resolve against exec's working
  # directory, which need not equal init's. An absolute path does not.
  bin_path="$(realpath_of "$bin_path" || printf '%s' "$bin_path")"
  bin_ver="$("$bin_path" --version 2>/dev/null | head -1 || true)"
  [[ -n "$bin_ver" ]] || bin_ver="unknown"
  tree_sha="$(cd "$REPO_ROOT" && git rev-parse --short HEAD 2>/dev/null || echo unknown)"

  # Two version shapes ship: `alpha-<stamp>-<sha7>` from the release
  # channel and `dev+g<sha7>[-dirty]` from a local build. Take the last
  # run of 7-or-more hex characters, which is the sha in both.
  bin_sha="$(printf '%s' "$bin_ver" | grep -oE '[0-9a-f]{7,}' | tail -1 || true)"
  [[ "$bin_ver" == *dirty* ]] && bin_dirty=1

  # `skew_basis` records WHICH test decided, and is written to run_start
  # whether the test passed or failed. Without it a passing dirty build
  # is indistinguishable from an unchecked one: run_start showed
  # `dev+g08a5350-dirty` against `repo_commit 06abc1b` with a null
  # skew_reason, and reading that mid-run I concluded the sha comparison
  # had been skipped by mistake and that the whole run had measured the
  # wrong document set. It had not. The comparison is skipped on purpose,
  # for the reason directly below, and the record should say so rather
  # than leave the next reader to reconstruct it from this source file.
  if [[ "$bin_dirty" -eq 1 ]]; then
    # A dirty build corresponds to NO commit, and build.rs does not
    # re-run on every HEAD move, so the sha inside a dev version is not
    # trustworthy either. Fall back to something that does not depend on
    # the binary's self-report: is any source file newer than the binary?
    local newest
    skew_basis="mtime: dirty build, so the sha in the version string is not a commit and is deliberately not compared"
    newest="$(find "$REPO_ROOT/crates" "$REPO_ROOT/spec" -type f -newer "$bin_path" -print -quit 2>/dev/null || true)"
    [[ -n "$newest" ]] && skew_reason="a source file changed after this binary was built: ${newest#"$REPO_ROOT"/}"
  elif [[ "$bin_sha" == "" ]]; then
    skew_basis="none: the version string names no commit to compare"
    skew_reason="the binary's version string names no commit: $bin_ver"
  else
    skew_basis="commit: binary $bin_sha against tree $tree_sha"
    if [[ "$tree_sha" != "unknown" && "$bin_sha" != "$tree_sha"* && "$tree_sha" != "$bin_sha"* ]]; then
      skew_reason="the binary was built from $bin_sha; the tree is at $tree_sha"
    fi
  fi

  if [[ -n "$skew_reason" && "$skew_ok" -ne 1 ]]; then
    cat >&2 <<EOF
runlog.sh: refusing to start. The binary and the tree disagree.

  binary   $bin_path
           $bin_ver
  tree     $tree_sha

  $skew_reason

A run records repo_commit from the tree and executes whatever \`stave\`
PATH resolves to. When those differ, every finding about what the tool
can reach is attributed to the wrong document set, and the judges'
surface.md describes the tree's documents rather than the ones that ran.

Build the tree's binary and put it first on PATH, or pass --allow-skew if
the skew is the point of the run. --allow-skew records the mismatch in
run_start; it does not hide it.
EOF
    exit "$EX_STATE"
  fi

  # Pin the binary this run executes to the one just resolved and
  # skew-checked. `exec` reads this rather than re-resolving on PATH,
  # which may have changed, or may no longer carry STAVE_BIN, mid-run.
  # The canonical command the coach reviews stays the basename (normally
  # `stave`); only which binary runs is recorded here. Single quotes in
  # the path are escaped so `load_run`'s `. run.env` cannot be broken by
  # a path like /opt/bob's-build/stave.
  printf "RUNLOG_BINARY_PATH='%s'\n" "${bin_path//\'/\'\\\'\'}" >> "$RUN_DIR/run.env"

  jq -n --arg sv "$tree_sha" --arg bp "$bin_path" --arg bv "$bin_ver" \
        --argjson skew "$([[ "$skew_ok" -eq 1 ]] && echo true || echo false)" \
        --argjson dirty "$([[ "$bin_dirty" -eq 1 ]] && echo true || echo false)" \
        --arg sr "$skew_reason" --arg sb "$skew_basis" \
        --arg audit "$AUDIT_DIR" --arg run_dir "$RUN_DIR" '
    {repo_commit: $sv, binary_path: $bp, binary_version: $bv,
     binary_dirty: $dirty, skew_allowed: $skew,
     skew_basis: (if $sb == "" then null else $sb end),
     skew_reason: (if $sr == "" then null else $sr end),
     audit_dir: $audit, run_dir: $run_dir}
  ' | append_entry run_start

  cat <<EOF
run initialised.

  run_dir    $RUN_DIR
  run_id     $RUN_ID
  session_id $SESSION_ID
  audit_dir  $AUDIT_DIR

Export these for the rest of the run:

  export STAVE_RUNLOG_DIR='$RUN_DIR'
EOF
}

# ---------------------------------------------------------------------
# canon, step, friction, result, dead-end, oob
# ---------------------------------------------------------------------

split_dashdash() {
  # Sets ARGV_ARR to everything after the first bare --.
  ARGV_ARR=()
  local seen=0 a
  for a in "$@"; do
    if [[ "$seen" -eq 1 ]]; then ARGV_ARR+=("$a")
    elif [[ "$a" == "--" ]]; then seen=1
    fi
  done
  [[ "$seen" -eq 1 ]] || return 1
  [[ "${#ARGV_ARR[@]}" -gt 0 ]] || return 1
  return 0
}

cmd_canon() {
  split_dashdash "$@" || die "canon: expected '-- <argv...>'"
  canon_argv "${ARGV_ARR[@]}"
}

cmd_step() {
  local step="" intent="" criterion=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --step) step="${2:?}"; shift 2 ;;
      --intent) intent="${2:?}"; shift 2 ;;
      --criterion) criterion="${2:?}"; shift 2 ;;
      *) die "step: unknown argument: $1" ;;
    esac
  done
  [[ -n "$step" ]] || die "step: --step is required"
  [[ -n "$intent" ]] || die "step: --intent is required (the operator's terms, not the tool's)"
  load_run
  printf '%s' "$step" > "$RUN_DIR/state/step"
  jq -n --arg i "$(scrub_text "$intent")" --arg c "$(scrub_text "$criterion")" '
    {intent: $i, criterion: (if $c == "" then null else $c end)}
  ' | append_entry step_start
}

cmd_friction() {
  local what="" cost="" category=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --what) what="${2:?}"; shift 2 ;;
      --cost) cost="${2:?}"; shift 2 ;;
      --category) category="${2:?}"; shift 2 ;;
      *) die "friction: unknown argument: $1" ;;
    esac
  done
  [[ -n "$what" ]] || die "friction: --what is required"
  load_run
  record_friction "$what" "$cost" "${category:-manual}"
}

record_friction() {
  jq -n --arg w "$(scrub_text "$1")" --arg c "$(scrub_text "${2:-}")" --arg k "${3:-manual}" '
    {what: $w, cost: (if $c == "" then null else $c end), category: $k}
  ' | append_entry friction
}

cmd_result() {
  local outcome="" gap=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --outcome) outcome="${2:?}"; shift 2 ;;
      --gap) gap="${2:?}"; shift 2 ;;
      *) die "result: unknown argument: $1" ;;
    esac
  done
  case "$outcome" in
    met|partial|unmet) ;;
    *) die "result: --outcome must be met, partial, or unmet" ;;
  esac
  if [[ "$outcome" != "met" && -z "$gap" ]]; then
    die "result: --gap is required for partial and unmet (name what is missing, in runbook terms)"
  fi
  load_run
  jq -n --arg o "$outcome" --arg g "$(scrub_text "$gap")" '
    {outcome: $o, gap: (if $g == "" then null else $g end),
     authority: "executor"}
  ' | append_entry step_result
}

cmd_dead_end() {
  local why="" desc=""
  local -a rest=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --why) why="${2:?}"; shift 2 ;;
      --description) desc="${2:?}"; shift 2 ;;
      --) shift; rest=("$@"); break ;;
      *) die "dead-end: unknown argument: $1" ;;
    esac
  done
  [[ -n "$why" ]] || die "dead-end: --why is required (why it was discarded)"
  load_run
  local cmd=""
  [[ "${#rest[@]}" -gt 0 ]] && cmd="$(canon_argv "${rest[@]}")"
  [[ -n "$cmd" || -n "$desc" ]] || die "dead-end: give '-- <argv>' or --description"
  jq -n --arg c "$(scrub_text "$cmd")" --arg d "$(scrub_text "$desc")" \
        --arg w "$(scrub_text "$why")" '
    {command: (if $c == "" then null else $c end),
     description: (if $d == "" then null else $d end),
     why: $w}
  ' | append_entry dead_end
}

# The four read-surface tickets. An out_of_band stage that does NOT
# survive the fix must name which one deletes it, because the whole
# reason this field exists is that the ticket's own motivating example
# was an instance of document debt read as verb demand.
SURFACE_TICKETS="rsh6 j1xi qijl gs23"

cmd_oob() {
  local tool="" purpose="" text="" survives="" reason=""
  local -a tickets=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --tool) tool="${2:?}"; shift 2 ;;
      --purpose) purpose="${2:?}"; shift 2 ;;
      --text) text="${2:?}"; shift 2 ;;
      --survives-fix) survives="${2:?}"; shift 2 ;;
      --reason) reason="${2:?}"; shift 2 ;;
      --ticket) tickets+=("${2:?}"); shift 2 ;;
      *) die "oob: unknown argument: $1" ;;
    esac
  done
  [[ -n "$tool" ]] || die "oob: --tool is required (jq, shell, manual, other)"
  [[ -n "$purpose" ]] || die "oob: --purpose is required (what it does to the data, not how)"
  case "$survives" in
    yes|no) ;;
    *) die "oob: --survives-fix must be yes or no.

Would this work still be necessary if aae-orc-rsh6, j1xi, qijl, and gs23
had landed? Without the answer the runlog records document debt as verb
demand, which is the exact false positive this exercise exists to avoid.
See docs/design/field-surface-audit.md." ;;
  esac
  if [[ "$survives" == "no" && "${#tickets[@]}" -eq 0 ]]; then
    die "oob: --survives-fix no requires at least one --ticket from: $SURFACE_TICKETS"
  fi
  local t
  for t in ${tickets[@]+"${tickets[@]}"}; do
    case " $SURFACE_TICKETS " in
      *" $t "*) ;;
      *) die "oob: unknown ticket '$t'; expected one of: $SURFACE_TICKETS" ;;
    esac
  done
  load_run
  local tj="[]"
  [[ "${#tickets[@]}" -gt 0 ]] && tj="$(printf '%s\n' "${tickets[@]}" | jq -R . | jq -sc .)"
  jq -n --arg tool "$tool" --arg p "$(scrub_text "$purpose")" \
        --arg txt "$(scrub_text "$text")" --arg s "$survives" \
        --arg r "$(scrub_text "$reason")" --argjson tk "$tj" '
    {tool: $tool, purpose: $p,
     text: (if $txt == "" then null else $txt end),
     survives_fix: ($s == "yes"),
     survives_fix_reason: (if $r == "" then null else $r end),
     deleted_by: $tk}
  ' | append_entry out_of_band
}

# ---------------------------------------------------------------------
# verdict
# ---------------------------------------------------------------------
#
# The gate. The coach is a Claude subagent and a shell script cannot
# invoke one, so the mechanism is a precondition rather than a call:
# `exec` will not run an invocation unless a verdict record exists whose
# COMMAND line matches the canonical text of that exact argv.
#
# What that buys, and what it does not, is set out in
# docs/design/runlog-harness.md under "The gate and its limits". The
# short version: this makes skipping the coach an act of forgery rather
# than an act of forgetting, and forgery is reviewable after the fact.
# It cannot prove a subagent was consulted.

cmd_verdict() {
  local coach_file=""
  local -a argv=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --coach-file) coach_file="${2:?}"; shift 2 ;;
      --) shift; argv=("$@"); break ;;
      *) die "verdict: unknown argument: $1" ;;
    esac
  done
  [[ -n "$coach_file" ]] || die "verdict: --coach-file is required (the coach's output block, or - for stdin)"
  [[ "${#argv[@]}" -gt 0 ]] || die "verdict: expected '-- <argv...>'"
  load_run

  local raw
  if [[ "$coach_file" == "-" ]]; then raw="$(cat)"
  else
    [[ -f "$coach_file" ]] || die "verdict: no such file: $coach_file" "$EX_STATE"
    raw="$(cat "$coach_file")"
  fi

  local canon sha raw_sha
  canon="$(canon_argv "${argv[@]}")"
  sha="$(sha256_text "$canon")"
  raw_sha="$(sha256_text "$raw")"

  local v cmd reason doubt resolve
  v="$(field_of "$raw" 'VERDICT')"
  cmd="$(field_of "$raw" 'COMMAND')"
  reason="$(field_of "$raw" 'REASON')"
  doubt="$(field_of "$raw" 'DOUBT')"
  resolve="$(field_of "$raw" 'TO RESOLVE')"

  case "$v" in
    CLEAR|HALT) ;;
    *) die "verdict: the coach block has no 'VERDICT: CLEAR' or 'VERDICT: HALT' line" ;;
  esac
  [[ -n "$reason" ]] || die "verdict: the coach block has no 'REASON:' line"
  if [[ "$cmd" != "$canon" ]]; then
    cat >&2 <<EOF
runlog.sh: the coach reviewed a different command.

  reviewed:  $cmd
  proposed:  $canon

These must match byte for byte. Show the coach exactly what
'runlog.sh canon -- <argv>' prints, and pass the same argv here.
EOF
    exit "$EX_GATE"
  fi

  if [[ "$v" == "HALT" ]]; then
    [[ -n "$doubt" ]] || die "verdict: a HALT must carry a 'DOUBT:' line, verbatim"
    [[ -n "$resolve" ]] || die "verdict: a HALT must carry a 'TO RESOLVE:' line"
  fi

  jq -n --arg vv "$v" --arg c "$canon" --arg sha "$sha" \
        --arg r "$(scrub_text "$reason")" --arg d "$(scrub_text "$doubt")" \
        --arg tr "$(scrub_text "$resolve")" --arg rs "$raw_sha" '
    {verdict: $vv, command: $c, command_sha256: $sha,
     reason: $r,
     doubt: (if $d == "" then null else $d end),
     to_resolve: (if $tr == "" then null else $tr end),
     coach_block_sha256: $rs}
  ' | append_entry coach_verdict

  if [[ "$v" == "HALT" ]]; then
    jq -n --arg c "$canon" --arg d "$(scrub_text "$doubt")" \
          --arg tr "$(scrub_text "$resolve")" '
      {command: $c, doubt: $d, to_resolve: $tr, disposition: "pending"}
    ' | append_entry halt
    {
      printf 'HALT on: %s\n' "$canon"
      printf 'DOUBT: %s\n' "$doubt"
      printf 'TO RESOLVE: %s\n' "$resolve"
    } > "$RUN_DIR/HALTED"
    cat >&2 <<EOF
runlog.sh: HALT recorded. The run is stopped, not just this step.

  $canon

DOUBT: $doubt
TO RESOLVE: $resolve

Take this to the human. They choose skip, proceed with a modification,
or stop. Record the ruling with 'runlog.sh resume'. Do not reformulate
around it quietly.
EOF
    exit "$EX_GATE"
  fi

  # CLEAR: write the single-use verdict record.
  jq -n --arg rid "$RUN_ID" --arg c "$canon" --arg sha "$sha" \
        --arg r "$(scrub_text "$reason")" --arg rs "$raw_sha" --arg ts "$(utc_now)" '
    {run_id: $rid, verdict: "CLEAR", command: $c, command_sha256: $sha,
     reason: $r, coach_block_sha256: $rs, recorded_at: $ts}
  ' > "$RUN_DIR/verdicts/$sha.json"
  printf 'CLEAR recorded for: %s\n' "$canon"
}

# Pull "KEY: value" out of the coach block, joining any continuation
# lines that follow it, since the coach's REASON is one or two sentences
# and may wrap. Stops at the next KEY: line, a blank line, or a fence.
field_of() {
  printf '%s\n' "$1" | awk -v key="$2" '
    BEGIN { want = key ": "; inblock = 0; acc = "" }
    {
      line = $0
      sub(/\r$/, "", line)
      if (index(line, want) == 1) {
        acc = substr(line, length(want) + 1)
        inblock = 1
        next
      }
      if (inblock) {
        if (line ~ /^[A-Z][A-Z ]*:/ || line ~ /^```/ || line ~ /^[[:space:]]*$/) { inblock = 0; next }
        sub(/^[[:space:]]+/, "", line)
        acc = acc " " line
      }
    }
    END {
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", acc)
      print acc
    }'
}

cmd_resume() {
  local disp="" ruling=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --disposition) disp="${2:?}"; shift 2 ;;
      --ruling) ruling="${2:?}"; shift 2 ;;
      *) die "resume: unknown argument: $1" ;;
    esac
  done
  case "$disp" in
    skip|proceed-modified|stop) ;;
    *) die "resume: --disposition must be skip, proceed-modified, or stop" ;;
  esac
  [[ -n "$ruling" ]] || die "resume: --ruling is required (what the human decided, in their words)"
  load_run
  [[ -f "$RUN_DIR/HALTED" ]] || die "resume: this run is not halted" "$EX_STATE"
  jq -n --arg d "$disp" --arg r "$(scrub_text "$ruling")" '
    {disposition: $d, human_ruling: $r,
     attested_by: "executor",
     note: "the harness cannot verify that a human made this ruling"}
  ' | append_entry halt
  rm -f "$RUN_DIR/HALTED"
  if [[ "$disp" == "stop" ]]; then
    jq -n '{reason: "stopped after a halt"}' | append_entry run_end
    printf 'run stopped.\n'
  else
    printf 'halt cleared (%s). The run may continue.\n' "$disp"
  fi
}

# ---------------------------------------------------------------------
# exec
# ---------------------------------------------------------------------

realpath_of() {
  local p="$1"
  if [[ -d "$p" ]]; then (cd "$p" && pwd -P)
  else (cd "$(dirname "$p")" 2>/dev/null && printf '%s/%s\n' "$(pwd -P)" "$(basename "$p")")
  fi
}

audit_snapshot() {
  : > "$1"
  [[ -d "$AUDIT_DIR" ]] || return 0
  local f
  for f in "$AUDIT_DIR"/*.jsonl; do
    [[ -e "$f" ]] || continue
    printf '%s\t%s\n' "$f" "$(wc -l < "$f" | tr -d ' ')" >> "$1"
  done
  return 0
}

audit_new_lines() {
  [[ -d "$AUDIT_DIR" ]] || return 0
  local f old
  for f in "$AUDIT_DIR"/*.jsonl; do
    [[ -e "$f" ]] || continue
    old="$(awk -F'\t' -v k="$f" '$1==k{print $2}' "$1")"
    [[ -n "$old" ]] || old=0
    tail -n +$((old + 1)) "$f"
  done
  return 0
}

cmd_exec() {
  local out_name="" in_path="" catalog=0
  local -a argv=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --out) out_name="${2:?}"; shift 2 ;;
      --in) in_path="${2:?}"; shift 2 ;;
      --catalog) catalog=1; shift ;;
      --) shift; argv=("$@"); break ;;
      *) die "exec: unknown argument: $1" ;;
    esac
  done
  [[ "${#argv[@]}" -gt 0 ]] || die "exec: expected '-- <argv...>'"
  load_run
  require_not_halted

  # The binary this run executes is the one init resolved and recorded.
  # Fall back to resolving STAVE_BIN_NAME for runs whose init predates the
  # recorded field. Either way the CANONICAL command and the coach's
  # verdict use the binary's basename (normally `stave`); STAVE_BIN's
  # directory only decides which binary that basename is, so a path-valued
  # STAVE_BIN never reaches the canon or the gate below.
  local exec_bin="$BINARY_PATH"
  if [[ -z "$exec_bin" ]]; then
    exec_bin="$(command -v "$STAVE_BIN_NAME" 2>/dev/null || true)"
    [[ -n "$exec_bin" ]] || die "exec: no '$STAVE_BIN_NAME' found to run" "$EX_STATE"
  fi
  # A run pins its binary at init; if it was moved, deleted, or rebuilt
  # away since, fail as a state error naming the binary rather than
  # letting the launch fail opaquely and be recorded as a command failure.
  [[ -x "$exec_bin" ]] || die "exec: run binary is not an executable file: $exec_bin" "$EX_STATE"

  # The gate is on the binary's basename. argv[0] the coach reviewed must
  # name the stave binary and nothing else — but compared by basename, so
  # a path-valued STAVE_BIN (or a path written into argv[0]) does not make
  # this gate unpassable. Before this fix a path-valued STAVE_BIN was
  # compared against a bare basename and could never match: bd aae-orc-98g6.
  local want_name
  want_name="$(basename "$exec_bin")"
  [[ "$(basename "${argv[0]}")" == "$want_name" ]] \
    || die "exec: this harness runs '$want_name' and nothing else; got '${argv[0]}'"

  local canon sha
  canon="$(canon_argv "${argv[@]}")"
  sha="$(sha256_text "$canon")"

  # ---- the gate ----
  local vfile="$RUN_DIR/verdicts/$sha.json"
  if [[ ! -f "$vfile" ]]; then
    cat >&2 <<EOF
runlog.sh: refusing to execute. No CLEAR verdict is on file for this
invocation.

  $canon

Hand exactly that text to the stave-safety-coach subagent, then record
what it returns:

  scripts/runlog.sh verdict --coach-file <file> -- ${argv[*]}

A verdict recorded after the fact is not a gate. This refusal is the
gate, per .claude/rules/safety-coach-gate.md.
EOF
    exit "$EX_GATE"
  fi
  local vrun
  vrun="$(jq -r '.run_id' < "$vfile")"
  [[ "$vrun" == "$RUN_ID" ]] || die "exec: that verdict belongs to a different run" "$EX_GATE"

  # Single use. The coach's own check 4 cares about repeats within a
  # session, so one CLEAR cannot license the same bulk pull twice.
  local vseq consumed vref
  vseq=0
  [[ -f "$RUN_DIR/state/vseq" ]] && vseq="$(cat "$RUN_DIR/state/vseq")"
  vseq=$((vseq + 1))
  printf '%s' "$vseq" > "$RUN_DIR/state/vseq"
  vref="$(jq -r '.coach_block_sha256' < "$vfile")"
  consumed="$RUN_DIR/verdicts/consumed/$sha.$vseq.json"
  mv "$vfile" "$consumed"

  # ---- provenance of every file argument ----
  local data_root a rp
  data_root="$(realpath_of "$RUN_DIR/data")"
  for a in "${argv[@]:1}"; do
    [[ -e "$a" ]] || continue
    rp="$(realpath_of "$a")"
    case "$rp" in
      "$data_root"/*) ;;
      *) die "exec: refusing a file argument from outside the run's scrubbed data dir: $a" "$EX_STATE" ;;
    esac
  done

  # ---- mode, decided by the VERB and never by a flag ----
  #
  # An earlier version keyed this on `--in` alone. That made `--in` a
  # scrubber bypass by combination: `--in <scrubbed file> -- stave list
  # issue` passes the gate (the coach reviews the canonical argv and
  # never sees a harness flag), ignores the stdin it was handed, reaches
  # the tenant anyway, and writes the raw answer to `data/`. Measured
  # 2026-08-07 against a stub: a person's name and an ARN survived intact.
  #
  # So: only the three stream verbs may run unscrubbed, and only `emit`
  # actually does. `filter` and `enrich` emit JSONL, the scrubber is
  # idempotent over already-scrubbed JSONL (verified), and re-scrubbing
  # costs nothing. `emit` renders, the field allowlist cannot classify a
  # rendered table, and its input provably came from `data/`.
  local verb="${argv[1]:-}"
  local mode="source"
  if [[ -n "$in_path" ]]; then
    case "$verb" in
      filter|enrich|emit) ;;
      *) die "exec: --in is for the stream verbs (filter, enrich, emit); '$verb' reaches the tenant, so its output is always scrubbed" ;;
    esac
    [[ -f "$in_path" ]] || die "exec: --in: no such file: $in_path" "$EX_STATE"
    rp="$(realpath_of "$in_path")"
    case "$rp" in
      "$data_root"/*) ;;
      *) die "exec: --in must name a file under $data_root (scrubbed by this harness)" "$EX_STATE" ;;
    esac
    case "$verb" in
      emit) mode="render" ;;
      *) mode="stream" ;;
    esac
  fi

  [[ -n "$out_name" ]] || out_name="seq${vseq}.out"
  case "$out_name" in
    */*|..|.|"") die "exec: --out is a plain file name, not a path" ;;
  esac
  local outfile="$RUN_DIR/data/$out_name"
  local errfile="$RUN_DIR/tmp/seq${vseq}.err"

  local snap="$RUN_DIR/tmp/seq${vseq}.audit-snapshot"
  audit_snapshot "$snap"

  export STAVE_SESSION_ID="$SESSION_ID"
  export STAVE_AUDIT_DIR="$AUDIT_DIR"

  local t0 t1 rc_cmd rc_scrub
  t0="$(epoch_ms)"
  # stdin comes from --in when there is one, in either mode. Scrubbing is
  # what the mode decides, and nothing else.
  local stdin_from="/dev/null"
  [[ -n "$in_path" ]] && stdin_from="$in_path"

  # Launch the configured binary, carrying the reviewed argv[1:] through
  # unchanged. argv[0] was only ever the logical name for the coach and
  # the record; the process itself is exec_bin, so the run executes the
  # binary init pinned rather than whatever PATH resolves argv[0] to.
  local -a run_argv=("$exec_bin")
  [[ "${#argv[@]}" -gt 1 ]] && run_argv+=("${argv[@]:1}")

  # `render` is the ONLY unscrubbed path, and it cannot reach the tenant.
  if [[ "$mode" != "render" ]]; then
    # Tenant output never touches a durable path unscrubbed: the raw
    # bytes exist only inside this pipe.
    local -a scrub_args=()
    [[ "$catalog" -eq 1 ]] && scrub_args+=(--catalog)
    set +e
    "${run_argv[@]}" < "$stdin_from" 2> "$errfile" | "$SCRUB" ${scrub_args[@]+"${scrub_args[@]}"} > "$outfile"
    local -a st=("${PIPESTATUS[@]}")
    set -e
    rc_cmd="${st[0]}"; rc_scrub="${st[1]}"
  else
    set +e
    "${run_argv[@]}" < "$stdin_from" 2> "$errfile" > "$outfile"
    rc_cmd=$?
    set -e
    rc_scrub=0
  fi
  t1="$(epoch_ms)"

  # Fail closed. A refusal means the scrubber could not classify the
  # shape, which is exactly when the field allowlist cannot be applied.
  if [[ "$rc_scrub" -ne 0 ]]; then
    rm -f "$outfile"
  fi

  local stderr_txt=""
  [[ -s "$errfile" ]] && stderr_txt="$(head -c 2000 "$errfile" | "$SCRUB" --text || true)"
  rm -f "$errfile"

  local out_lines=0 out_bytes=0
  if [[ -f "$outfile" ]]; then
    out_lines="$(wc -l < "$outfile" | tr -d ' ')"
    out_bytes="$(wc -c < "$outfile" | tr -d ' ')"
  fi

  # Join keys, read back out of stave's own audit trail. Only join keys
  # and enums are lifted; argv, variables, and cursors stay where they
  # are, because those are the tenant-identifying half.
  local newaudit="$RUN_DIR/tmp/seq${vseq}.audit-new"
  audit_new_lines "$snap" > "$newaudit" || true
  local traces ops results audit_n
  traces="$(jq -sc --arg sid "$SESSION_ID" '[.[] | select((.invocation.session_id // "") == $sid) | .trace_id] | unique' < "$newaudit" 2>/dev/null || echo '[]')"
  ops="$(jq -sc '[.[] | .operation.id // empty] | unique' < "$newaudit" 2>/dev/null || echo '[]')"
  results="$(jq -sc '[.[] | .result // empty] | unique' < "$newaudit" 2>/dev/null || echo '[]')"
  audit_n="$(wc -l < "$newaudit" | tr -d ' ')"
  rm -f "$snap" "$newaudit"

  jq -n --arg c "$canon" --arg sha "$sha" --arg mode "$mode" \
        --argjson rc "$rc_cmd" --argjson rs "$rc_scrub" \
        --argjson dur "$((t1 - t0))" \
        --arg outp "data/$out_name" --argjson ol "$out_lines" --argjson ob "$out_bytes" \
        --argjson tr "$traces" --argjson ops "$ops" --argjson res "$results" \
        --argjson an "$audit_n" --arg se "$stderr_txt" --arg vref "$vref" '
    {command: $c, command_sha256: $sha, mode: $mode,
     verdict_ref: $vref,
     exit_code: $rc, scrub_exit: $rs, duration_ms: $dur,
     output_path: (if $rs == 0 then $outp else null end),
     output_lines: $ol, output_bytes: $ob,
     trace_ids: $tr, operations: $ops, results: $res,
     audit_lines: $an,
     stderr_excerpt: (if $se == "" then null else $se end)}
  ' | append_entry stave_call

  if [[ "$rc_scrub" -ne 0 ]]; then
    record_friction \
      "the scrubber refused this output shape (exit $rc_scrub); the output was discarded" \
      "the step produced nothing usable" "scrub_refused"
    cat >&2 <<EOF
runlog.sh: the scrubber refused this output and nothing was written.

Most often this is a rendered table: scrub BEFORE emit, not after.
  ... | scripts/runlog.sh exec --in <scrubbed>.jsonl -- stave emit --format md
EOF
    exit "$EX_SCRUB"
  fi
  if [[ "$rc_cmd" -ne 0 ]]; then
    printf 'runlog.sh: the invocation exited %s (recorded).\n' "$rc_cmd" >&2
    exit "$EX_CMD"
  fi
  printf '%s\n' "$outfile"
}

# ---------------------------------------------------------------------
# finish, reconcile
# ---------------------------------------------------------------------

cmd_finish() {
  local note=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --note) note="${2:?}"; shift 2 ;;
      *) die "finish: unknown argument: $1" ;;
    esac
  done
  load_run
  jq -n --arg n "$(scrub_text "$note")" '
    {reason: "finished", note: (if $n == "" then null else $n end)}
  ' | append_entry run_end
  cmd_reconcile
}

# The join this exercise exists to justify, applied to the exercise.
# Reports only join keys and enums, so its output is safe to read.
cmd_reconcile() {
  [[ -n "$RUN_ID" ]] || load_run
  local ta tr
  ta="$RUN_DIR/tmp/reconcile.audit"
  tr="$RUN_DIR/tmp/reconcile.runlog"
  cat "$AUDIT_DIR"/*.jsonl 2>/dev/null \
    | jq -r --arg sid "$SESSION_ID" 'select((.invocation.session_id // "") == $sid) | .trace_id' \
    | sort -u > "$ta" || : > "$ta"
  jq -r 'select(.type == "stave_call") | .trace_ids[]?' < "$RUN_DIR/runlog.jsonl" \
    | sort -u > "$tr"

  local n_audit n_runlog matched audit_only runlog_only
  n_audit="$(wc -l < "$ta" | tr -d ' ')"
  n_runlog="$(wc -l < "$tr" | tr -d ' ')"
  matched="$(comm -12 "$ta" "$tr" | wc -l | tr -d ' ')"
  audit_only="$(comm -23 "$ta" "$tr" | wc -l | tr -d ' ')"
  runlog_only="$(comm -13 "$ta" "$tr" | wc -l | tr -d ' ')"

  cat <<EOF

reconcile: runlog x audit, joined on session_id then trace_id

  session_id        $SESSION_ID
  audit traces      $n_audit
  runlog traces     $n_runlog
  matched           $matched
  audit only        $audit_only   <- invocations that bypassed the harness
  runlog only       $runlog_only   <- recorded calls with no audit line
EOF

  if [[ "$audit_only" -gt 0 ]]; then
    cat <<EOF

  An audit line carrying this run's session_id with no matching
  stave_call means stave ran outside the harness, so it ran without a
  coach verdict and its output was never scrubbed. Traces:

$(comm -23 "$ta" "$tr" | sed 's/^/    /')
EOF
    return 1
  fi
  return 0
}

# ---------------------------------------------------------------------
# selftest
# ---------------------------------------------------------------------
#
# Synthetic values only. No tenant, no credentials, no network, and the
# real stave binary is never invoked: argv[0] is a stub on PATH.

selftest() {
  local fail=0
  SELFTEST_ROOT="$(mktemp -d)"
  trap 'rm -rf "$SELFTEST_ROOT"' EXIT
  local root="$SELFTEST_ROOT"

  ok_() { printf 'ok   %-46s\n' "$1"; }
  bad_() { printf 'FAIL %-46s %s\n' "$1" "${2:-}" >&2; fail=1; }

  # --- the stub -----------------------------------------------------
  mkdir -p "$root/bin"
  cat > "$root/bin/stave" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
# --version first, and before the ran-marker: init calls it to identify
# the binary, and that is bookkeeping rather than an executed invocation.
if [[ "${1:-}" == "--version" ]]; then
  printf 'stave 0.0.1 (alpha-selftest-%s)\n' "${STUB_VERSION_SHA:-0000000}"
  exit 0
fi
printf 'ran\n' >> "${STUB_RAN_MARKER:?}"
mkdir -p "${STAVE_AUDIT_DIR:?}"
trace="0198a2c1-7f3e-7c21-9b04-$(printf '%012d' "$(( RANDOM * RANDOM % 999999999 ))")"
case "${1:-}${2:+ $2}" in
  "list issue")
    jq -nc --arg t "$trace" --arg s "${STAVE_SESSION_ID:-}" \
      '{schema_version:3, trace_id:$t, invocation:{session_id:$s},
        operation:{id:"list_issues"}, result:"ok"}' >> "$STAVE_AUDIT_DIR/day.jsonl"
    cat <<'JSONL'
{"_kind":"issue","severity":"CRITICAL","status":"OPEN","id":"iss-1","entitySnapshot":{"type":"USER_ACCOUNT","name":"Jane Q Example","id":"arn:aws:iam::123456789012:role/ExampleRole"}}
{"_kind":"issue","severity":"HIGH","status":"OPEN","id":"iss-2","entitySnapshot":{"type":"VM","cloudPlatform":"AWS","name":"prod-db-primary"}}
JSONL
    ;;
  "emit --format")
    printf '| _kind | id |\n|---|---|\n'
    jq -r '"| \(._kind) | \(.id) |"' 2>/dev/null || true
    ;;
  "filter --where")
    cat
    ;;
  *) printf 'stub: unsupported: %s\n' "$*" >&2; exit 9 ;;
esac
STUB
  chmod +x "$root/bin/stave"
  export PATH="$root/bin:$PATH"
  export STUB_RAN_MARKER="$root/ran"
  : > "$STUB_RAN_MARKER"

  local rd="$root/run"
  # 0. A binary whose version does not name the tree's commit is refused,
  #    and no run directory survives. Commissioning run 1 spent a real
  #    tenant read before anyone noticed the skew.
  rc=0
  STUB_VERSION_SHA=deadbee "$0" init --runbook A1 --run-dir "$root/skew" >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq "$EX_STATE" ]]; then
    ok_ "version: a binary that does not match the tree is refused"
  else
    bad_ "version: a binary that does not match the tree is refused" "rc=$rc"
  fi
  rm -rf "$root/skew"

  # The stub is not stave, so the rest of the selftest runs under the
  # documented escape rather than by faking a matching version.
  "$0" init --runbook A1 --run-dir "$rd" --allow-skew >/dev/null
  jq -e 'select(.type=="run_start") | .skew_allowed == true and (.binary_version | test("alpha-selftest"))' \
    < "$rd/runlog.jsonl" >/dev/null \
    && ok_ "version: binary identity and the skew waiver are recorded" \
    || bad_ "version: binary identity and the skew waiver are recorded"

  # skew_basis names the test that decided, so a passing check is not
  # mistaken for an absent one. The stub reports a clean alpha version,
  # so the commit branch is the one that must be named here.
  jq -e 'select(.type=="run_start") | (.skew_basis // "") | startswith("commit:")' \
    < "$rd/runlog.jsonl" >/dev/null \
    && ok_ "version: skew_basis names which test decided" \
    || bad_ "version: skew_basis names which test decided"
  export STAVE_RUNLOG_DIR="$rd"

  coach() { # $1 verdict, $2 command, [$3 doubt]
    printf 'VERDICT: %s\nCOMMAND: %s\nREASON: selftest fixture.\n' "$1" "$2"
    [[ "$1" == "HALT" ]] && printf 'DOUBT: %s\nTO RESOLVE: %s\n' "${3:-synthetic}" "ask the human"
    return 0
  }

  local rc canon

  # 1. No verdict on file: refuses, and the stub is never run.
  rc=0; "$0" exec --out a.jsonl -- stave list issue --limit 2 >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq "$EX_GATE" && ! -s "$STUB_RAN_MARKER" ]]; then
    ok_ "gate: no verdict, nothing executed"
  else
    bad_ "gate: no verdict, nothing executed" "rc=$rc ran=$(wc -l < "$STUB_RAN_MARKER")"
  fi

  # 2. A verdict for a different command does not license this one.
  canon="$("$0" canon -- stave list project --limit 2)"
  coach CLEAR "$canon" | "$0" verdict --coach-file - -- stave list project --limit 2 >/dev/null
  rc=0; "$0" exec --out b.jsonl -- stave list issue --limit 2 >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq "$EX_GATE" && ! -s "$STUB_RAN_MARKER" ]]; then
    ok_ "gate: verdict for another command refused"
  else
    bad_ "gate: verdict for another command refused" "rc=$rc"
  fi

  # 3. A coach block whose COMMAND line disagrees is refused outright.
  rc=0
  coach CLEAR "stave list issue --limit 999" \
    | "$0" verdict --coach-file - -- stave list issue --limit 2 >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_GATE" ]] && ok_ "gate: coach reviewed a different command" \
    || bad_ "gate: coach reviewed a different command" "rc=$rc"

  # 4. Matching CLEAR executes, and nothing unscrubbed lands.
  canon="$("$0" canon -- stave list issue --limit 2)"
  coach CLEAR "$canon" | "$0" verdict --coach-file - -- stave list issue --limit 2 >/dev/null
  rc=0; "$0" exec --out issues.jsonl -- stave list issue --limit 2 >/dev/null || rc=$?
  [[ "$rc" -eq 0 && -s "$rd/data/issues.jsonl" ]] \
    && ok_ "exec: CLEAR executes and writes output" \
    || bad_ "exec: CLEAR executes and writes output" "rc=$rc"

  local leaked=0 needle
  for needle in 'Jane Q Example' 'prod-db-primary' 'arn:aws:iam::123456789012:role/ExampleRole'; do
    if grep -rqF -- "$needle" "$rd/data" "$rd/runlog.jsonl" 2>/dev/null; then
      bad_ "scrub: '$needle' reached a durable artifact"; leaked=1
    fi
  done
  [[ "$leaked" -eq 0 ]] && ok_ "scrub: no planted record reached data/ or runlog"

  # Positive control: the scrubber ran, it did not merely delete.
  grep -q 'CRITICAL' "$rd/data/issues.jsonl" \
    && ok_ "scrub: safe fields survived (positive control)" \
    || bad_ "scrub: safe fields survived (positive control)"

  # 5. A verdict is single use.
  rc=0; "$0" exec --out again.jsonl -- stave list issue --limit 2 >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_GATE" ]] && ok_ "gate: a CLEAR verdict is single use" \
    || bad_ "gate: a CLEAR verdict is single use" "rc=$rc"

  # 6. An unscrubbable shape fails closed.
  canon="$("$0" canon -- stave emit --format md)"
  coach CLEAR "$canon" | "$0" verdict --coach-file - -- stave emit --format md >/dev/null
  rc=0; "$0" exec --out table.md -- stave emit --format md >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq "$EX_SCRUB" && ! -e "$rd/data/table.md" ]]; then
    ok_ "scrub: unclassifiable shape fails closed, no file"
  else
    bad_ "scrub: unclassifiable shape fails closed, no file" "rc=$rc"
  fi
  jq -e 'select(.type=="friction" and .category=="scrub_refused")' < "$rd/runlog.jsonl" >/dev/null \
    && ok_ "scrub: refusal recorded as friction" \
    || bad_ "scrub: refusal recorded as friction"

  # 6b. `--in` is not a scrubber bypass. A tenant-reaching verb declared
  #     with `--in` is refused outright, because the coach reviews the
  #     canonical argv and never sees a harness flag: `--in <scrubbed> --
  #     stave list issue` would pass the gate, ignore the stdin it was
  #     handed, reach the tenant, and write the raw answer to data/.
  #     Measured against this stub on 2026-08-07, before the fix.
  : > "$STUB_RAN_MARKER"
  canon="$("$0" canon -- stave list issue --limit 2)"
  coach CLEAR "$canon" | "$0" verdict --coach-file - -- stave list issue --limit 2 >/dev/null
  rc=0
  "$0" exec --in "$rd/data/issues.jsonl" --out sneak.jsonl -- stave list issue --limit 2 \
    >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq "$EX_USAGE" && ! -e "$rd/data/sneak.jsonl" && ! -s "$STUB_RAN_MARKER" ]]; then
    ok_ "scrub: --in cannot turn a source verb unscrubbed"
  else
    bad_ "scrub: --in cannot turn a source verb unscrubbed" "rc=$rc"
  fi

  # 6c. A stream verb IS re-scrubbed, because the scrubber is idempotent
  #     over already-scrubbed JSONL and re-running it costs nothing.
  canon="$("$0" canon -- stave filter --where 'severity == "CRITICAL"')"
  coach CLEAR "$canon" | "$0" verdict --coach-file - -- stave filter --where 'severity == "CRITICAL"' >/dev/null
  rc=0
  "$0" exec --in "$rd/data/issues.jsonl" --out filtered.jsonl \
    -- stave filter --where 'severity == "CRITICAL"' >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq 0 ]] && grep -q '<redacted:' "$rd/data/filtered.jsonl" \
     && ! grep -q 'prod-db-primary' "$rd/data/filtered.jsonl"; then
    ok_ "scrub: a stream verb is scrubbed, not exempted"
  else
    bad_ "scrub: a stream verb is scrubbed, not exempted" "rc=$rc"
  fi
  jq -e 'select(.type=="stave_call" and .mode=="stream")' < "$rd/runlog.jsonl" >/dev/null \
    && ok_ "mode: stream recorded distinctly from source" \
    || bad_ "mode: stream recorded distinctly from source"

  # 6d. `emit` renders, so it alone is exempt — and what it renders is
  #     whatever its already-scrubbed input carried.
  canon="$("$0" canon -- stave emit --format md)"
  coach CLEAR "$canon" | "$0" verdict --coach-file - -- stave emit --format md >/dev/null
  rc=0
  "$0" exec --in "$rd/data/filtered.jsonl" --out report.md \
    -- stave emit --format md >/dev/null 2>&1 || rc=$?
  if [[ "$rc" -eq 0 ]] && grep -q '<redacted:id>' "$rd/data/report.md"; then
    ok_ "render: emit is exempt, and carries its input's scrubbing"
  else
    bad_ "render: emit is exempt, and carries its input's scrubbing" "rc=$rc"
  fi

  # 7. A HALT latches the whole run.
  canon="$("$0" canon -- stave list issue --limit 3)"
  rc=0
  coach HALT "$canon" "search walks the whole connection" \
    | "$0" verdict --coach-file - -- stave list issue --limit 3 >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_GATE" && -f "$rd/HALTED" ]] && ok_ "halt: recorded and latched" \
    || bad_ "halt: recorded and latched" "rc=$rc"

  canon="$("$0" canon -- stave list issue --limit 4)"
  coach CLEAR "$canon" | "$0" verdict --coach-file - -- stave list issue --limit 4 >/dev/null 2>&1 || true
  rc=0; "$0" exec --out halted.jsonl -- stave list issue --limit 4 >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_STATE" ]] && ok_ "halt: blocks a later CLEAR invocation" \
    || bad_ "halt: blocks a later CLEAR invocation" "rc=$rc"

  "$0" resume --disposition skip --ruling "synthetic ruling" >/dev/null
  [[ ! -f "$rd/HALTED" ]] && ok_ "halt: resume clears the latch and records it" \
    || bad_ "halt: resume clears the latch and records it"

  # 8. out_of_band demands the survives-fix answer, and a ticket with it.
  rc=0; "$0" oob --tool jq --purpose "group and count" --text "jq -s ..." >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_USAGE" ]] && ok_ "oob: --survives-fix is mandatory" \
    || bad_ "oob: --survives-fix is mandatory" "rc=$rc"
  rc=0; "$0" oob --tool jq --purpose "group and count" --survives-fix no >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_USAGE" ]] && ok_ "oob: survives-fix no needs a ticket" \
    || bad_ "oob: survives-fix no needs a ticket" "rc=$rc"
  rc=0; "$0" oob --tool jq --purpose "group and count" --survives-fix no --ticket nope >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_USAGE" ]] && ok_ "oob: unknown ticket rejected" \
    || bad_ "oob: unknown ticket rejected" "rc=$rc"
  "$0" oob --tool jq --purpose "group and count on a composite key" \
        --survives-fix no --ticket gs23 --text "jq -s 'group_by(...)'" >/dev/null
  jq -e 'select(.type=="out_of_band" and .survives_fix==false and (.deleted_by|index("gs23")))' \
     < "$rd/runlog.jsonl" >/dev/null \
    && ok_ "oob: recorded with survives_fix and deleted_by" \
    || bad_ "oob: recorded with survives_fix and deleted_by"

  # 9. A file argument from outside the scrubbed data dir is refused.
  printf '{}\n' > "$root/outside.jsonl"
  canon="$("$0" canon -- stave list issue --limit 2 "$root/outside.jsonl")"
  coach CLEAR "$canon" | "$0" verdict --coach-file - -- stave list issue --limit 2 "$root/outside.jsonl" >/dev/null
  rc=0; "$0" exec -- stave list issue --limit 2 "$root/outside.jsonl" >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_STATE" ]] && ok_ "provenance: outside file argument refused" \
    || bad_ "provenance: outside file argument refused" "rc=$rc"

  # 10. reconcile detects an invocation that bypassed the harness.
  rc=0; "$0" reconcile >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq 0 ]] && ok_ "reconcile: clean run reconciles" \
    || bad_ "reconcile: clean run reconciles" "rc=$rc"
  ( # shellcheck disable=SC1091
    . "$rd/run.env"
    export STAVE_AUDIT_DIR="$rd/audit" STAVE_SESSION_ID="$RUNLOG_SESSION_ID"
    stave list issue >/dev/null 2>&1 || true )
  rc=0; "$0" reconcile >/dev/null 2>&1 || rc=$?
  [[ "$rc" -ne 0 ]] && ok_ "reconcile: bypass detected" \
    || bad_ "reconcile: bypass detected" "rc=$rc"

  # 11. STAVE_BIN as a PATH. bd aae-orc-98g6: the exec gate compared a
  #     bare basename against the whole path and could never match, so a
  #     path-valued STAVE_BIN made exec unpassable; and had it passed,
  #     exec ran whatever `stave` resolved to on PATH while init recorded
  #     the configured binary, so the record disagreed with what ran.
  #     The binary lives OUTSIDE PATH, and a DIFFERENT `stave` sits first
  #     on PATH, so "runs the configured one" and "runs PATH's" are
  #     distinguishable by which marker file gets touched.
  mkdir -p "$root/altbin"
  cat > "$root/altbin/stave" <<'ALT'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--version" ]]; then
  printf 'stave 0.0.1 (alpha-altbin-%s)\n' "${STUB_VERSION_SHA:-0000000}"; exit 0
fi
printf 'ran\n' >> "${ALT_RAN_MARKER:?}"
mkdir -p "${STAVE_AUDIT_DIR:?}"
trace="0198a2c1-7f3e-7c21-9b04-$(printf '%012d' "$(( RANDOM * RANDOM % 999999999 ))")"
case "${1:-}${2:+ $2}" in
  "list issue")
    jq -nc --arg t "$trace" --arg s "${STAVE_SESSION_ID:-}" \
      '{schema_version:3, trace_id:$t, invocation:{session_id:$s},
        operation:{id:"list_issues"}, result:"ok"}' >> "$STAVE_AUDIT_DIR/day.jsonl"
    printf '{"_kind":"issue","id":"iss-alt","severity":"CRITICAL","status":"OPEN"}\n' ;;
  *) printf 'altbin: unsupported: %s\n' "$*" >&2; exit 9 ;;
esac
ALT
  chmod +x "$root/altbin/stave"
  local altrd="$root/run-altbin"
  export ALT_RAN_MARKER="$root/alt-ran"; : > "$ALT_RAN_MARKER"
  : > "$STUB_RAN_MARKER"

  # init pins the run to the path-valued binary. It records the absolute,
  # symlink-resolved path (mktemp lives under a /var -> /private/var
  # symlink on macOS), so the expectation is canonicalized the same way.
  local altbin_canon
  altbin_canon="$(cd "$root/altbin" && pwd -P)/stave"
  STAVE_BIN="$root/altbin/stave" "$0" init --runbook A1 --run-dir "$altrd" --allow-skew >/dev/null
  jq -e --arg bp "$altbin_canon" 'select(.type=="run_start") | .binary_path == $bp' \
    < "$altrd/runlog.jsonl" >/dev/null \
    && ok_ "stave-bin: a path-valued STAVE_BIN is recorded as the run binary" \
    || bad_ "stave-bin: a path-valued STAVE_BIN is recorded as the run binary"

  # The canonical command stays the logical name, and STAVE_BIN need not
  # even be re-exported: exec runs the binary init pinned in run.env.
  rc=0
  (
    export STAVE_RUNLOG_DIR="$altrd"
    c="$("$0" canon -- stave list issue --limit 2)"
    coach CLEAR "$c" | "$0" verdict --coach-file - -- stave list issue --limit 2 >/dev/null
    "$0" exec --out alt.jsonl -- stave list issue --limit 2 >/dev/null 2>&1
  ) || rc=$?
  if [[ "$rc" -eq 0 && -s "$ALT_RAN_MARKER" && ! -s "$STUB_RAN_MARKER" && -s "$altrd/data/alt.jsonl" ]]; then
    ok_ "stave-bin: path-valued STAVE_BIN execs, and runs that binary not PATH's"
  else
    bad_ "stave-bin: path-valued STAVE_BIN execs, and runs that binary not PATH's" \
      "rc=$rc alt=$(wc -c <"$ALT_RAN_MARKER" 2>/dev/null || echo 0) stub=$(wc -c <"$STUB_RAN_MARKER" 2>/dev/null || echo 0)"
  fi

  if [[ "$fail" -ne 0 ]]; then
    printf '\nrunlog.sh selftest FAILED\n' >&2
    exit 1
  fi
  printf '\nrunlog.sh selftest passed\n'
}

# ---------------------------------------------------------------------
# dispatch
# ---------------------------------------------------------------------

main() {
  need jq; need perl; need awk
  local sub="${1:-}"
  [[ $# -gt 0 ]] && shift || true

  # A --run-dir before the '--' wins over the environment. After the
  # '--' every token belongs to the invocation being reviewed, so
  # nothing there is interpreted.
  local -a rest=()
  local past=0
  while [[ $# -gt 0 ]]; do
    if [[ "$past" -eq 1 ]]; then rest+=("$1"); shift; continue; fi
    case "$1" in
      --) past=1; rest+=("$1"); shift ;;
      --run-dir) resolve_run_dir "${2:?}"; shift 2 ;;
      *) rest+=("$1"); shift ;;
    esac
  done
  [[ -n "$RUN_DIR" ]] || RUN_DIR="${STAVE_RUNLOG_DIR:-}"
  set -- ${rest[@]+"${rest[@]}"}

  case "$sub" in
    init)
      if [[ -n "${RUN_DIR:-}" ]]; then cmd_init --run-dir "$RUN_DIR" "$@"
      else cmd_init "$@"
      fi ;;
    canon)     cmd_canon "$@" ;;
    step)      cmd_step "$@" ;;
    verdict)   cmd_verdict "$@" ;;
    exec)      cmd_exec "$@" ;;
    oob)       cmd_oob "$@" ;;
    dead-end)  cmd_dead_end "$@" ;;
    result)    cmd_result "$@" ;;
    friction)  cmd_friction "$@" ;;
    resume)    cmd_resume "$@" ;;
    finish)    cmd_finish "$@" ;;
    reconcile) cmd_reconcile "$@" ;;
    selftest)  selftest ;;
    -h|--help|"") sed -n '2,50p' "$0" | sed 's/^# \{0,1\}//' ;;
    *) die "unknown subcommand: $sub" ;;
  esac
}

main "$@"
