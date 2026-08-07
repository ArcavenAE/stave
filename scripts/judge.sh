#!/usr/bin/env bash
# judge.sh: assemble judge packets, record judge verdicts, and compute
# the judge-versus-executor divergence.
#
# bd aae-orc-e4jo.10. Companion to scripts/runlog.sh. Design and the
# reasoning behind every allowlist decision: docs/design/runbook-judges.md.
#
# One property holds BY CONSTRUCTION rather than by anyone remembering:
#
#   THE JUDGE NEVER SEES THE EXECUTOR'S ACCOUNT OF ITSELF. `packet`
#   projects runlog.jsonl through a default-deny allowlist on entry TYPE
#   and, within a permitted type, on FIELD. step_result, friction,
#   dead_end and out_of_band are dropped whole. There is no flag that
#   includes them.
#
# The limits are documented, at length, in docs/design/runbook-judges.md
# section 4. Read that before trusting the property past what it claims.
#
# Usage:
#   scripts/judge.sh packet   --run-dir DIR [--judge NAME] [--out DIR]
#   scripts/judge.sh verdict  --packet DIR --file F|-
#   scripts/judge.sh diverge  [--runs-root DIR] [--json]
#   scripts/judge.sh assignments
#   scripts/judge.sh selftest

set -euo pipefail

JUDGE_SCHEMA_VERSION=1

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$HERE")"
SCRUB="$HERE/scrub.sh"

DOC_JUDGES="$REPO_ROOT/docs/design/runbook-judges.md"
DOC_CATALOGUE="$REPO_ROOT/docs/runbooks/catalogue.md"
DOC_SURFACE="$REPO_ROOT/docs/design/field-surface-audit.md"

EX_USAGE=2       # argv or usage error
EX_STATE=3       # run state wrong, or a path outside the run tree
EX_SCHEMA=4      # a verdict failed validation; nothing was written

die() { printf 'judge.sh: %s\n' "$1" >&2; exit "${2:-$EX_USAGE}"; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing dependency: $1"; }

sha256_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1
  fi
}
sha256_file() { sha256_stdin < "$1"; }
utc_now() { date -u +%Y-%m-%dT%H:%M:%SZ; }

# Free text a judge writes goes through the scrubber's pattern tier, the
# same backstop runlog.sh applies to executor free text. It catches
# emails, GUIDs, ARNs, OCIDs, IPs, account ids and the local literals. It
# does not catch a person's name or a bucket name: those have no shape.
scrub_text() {
  local s="$1"
  [[ -n "$s" ]] || { printf ''; return 0; }
  printf '%s' "$s" | "$SCRUB" --text
}

# ---------------------------------------------------------------------
# Assignment
# ---------------------------------------------------------------------
#
# Judge of record per runbook. The rule is mechanical: the first name in
# the originating-persona column of docs/runbooks/catalogue-provenance.md
# is the judge of record, and a second name is a recorded dissent that is
# not invoked by default. Mechanical because a rule chosen per case can
# be tuned per case. B11 is the one runbook where the mechanical rule and
# the plain reading of the column disagree; see section 3 of the design
# document, where the disagreement is recorded rather than resolved.
#
# selftest asserts this table agrees with the table in the design
# document, so the two cannot drift.

ASSIGN_RUNBOOKS=(A1 A2 A3 A4 A5 B6 B7 B8 B9 B10 B11 B12 B13 C14 C15 C16 C17 C18 C19 C20)
ASSIGN_JUDGES=(priya priya priya marcus greta ines dale renata greta marcus \
               marcus priya dale deepak sanne kwame ines deepak deepak kwame)

judge_for() {
  local rb="$1" i
  for i in "${!ASSIGN_RUNBOOKS[@]}"; do
    if [[ "${ASSIGN_RUNBOOKS[$i]}" == "$rb" ]]; then
      printf '%s\n' "${ASSIGN_JUDGES[$i]}"; return 0
    fi
  done
  return 1
}

cmd_assignments() {
  local i
  for i in "${!ASSIGN_RUNBOOKS[@]}"; do
    printf '%s\t%s\n' "${ASSIGN_RUNBOOKS[$i]}" "${ASSIGN_JUDGES[$i]}"
  done
}

# ---------------------------------------------------------------------
# The allowlist
# ---------------------------------------------------------------------
#
# Default-deny on two axes. An entry type absent from this map is
# dropped whole; a field absent from its type's list is dropped.
#
# Dropped whole, and why each one would tell the judge what to conclude:
#
#   step_result   the executor's own met/partial/unmet plus its named
#                 gap. This is the thing the judge is being asked to
#                 produce independently, and the divergence between the
#                 two is the measurement.
#   friction      the executor's account of what cost it something.
#   dead_end      `why` is the executor's explanation of a failure,
#                 which is exactly the EXECUTOR-SHORTFALL-versus-
#                 TOOL-CANNOT call the judge has to make unaided.
#   out_of_band   `purpose` and `survives_fix` are the executor's
#                 judgement about whether a hand stage is tool debt.
#                 Mining consumes these (aae-orc-e4jo.7); the judge
#                 must not.
#
# Dropped fields inside permitted types:
#
#   step_start.intent, .criterion   the executor's framing and its
#                 transcription of the criterion. The packet carries the
#                 criterion from catalogue.md instead, so a transcription
#                 that drifted is visible as a difference rather than
#                 adopted as the standard.
#   run_end.note  executor free text.
#   halt.human_ruling, .doubt, .to_resolve   the disposition enum says a
#                 step was gated; the prose says what someone thought
#                 about it.
#   stave_call.trace_ids, .command_sha256, .verdict_ref, .audit_lines
#                 join keys into audit/, which is unscrubbed. The judge
#                 is never pointed at the audit trail.
#   stave_call.duration_ms   no bearing on satisfaction, and it invites
#                 reading slow as bad.

ALLOW_MAP='{
  "run_start":    ["seq","ts","type","runbook","step","run_id","repo_commit"],
  "step_start":   ["seq","ts","type","runbook","step"],
  "coach_verdict":["seq","ts","type","runbook","step","verdict","command","reason"],
  "stave_call":   ["seq","ts","type","runbook","step","command","mode","exit_code",
                   "scrub_exit","operations","results","output_path","output_lines",
                   "stderr_excerpt"],
  "halt":         ["seq","ts","type","runbook","step","disposition"],
  "run_end":      ["seq","ts","type","runbook","step","reason"]
}'

EXCLUDED_TYPES='["step_result","friction","dead_end","out_of_band"]'

project_runlog() {
  jq -c --argjson allow "$ALLOW_MAP" '
    select(.type as $t | ($allow | has($t)))
    | . as $e
    | ($allow[$e.type]) as $keys
    | reduce $keys[] as $k ({}; if ($e | has($k)) then . + {($k): $e[$k]} else . end)
  ' "$1"
}

# ---------------------------------------------------------------------
# Slicing the reference documents
# ---------------------------------------------------------------------

slice_anchor() { # file, anchor name -> content between <!-- x --> and <!-- /x -->
  awk -v a="$2" '
    $0 == "<!-- " a " -->" { on=1; next }
    $0 == "<!-- /" a " -->" { on=0 }
    on { print }
  ' "$1"
}

slice_runbook() { # catalogue.md, runbook id
  awk -v rb="$2" '
    $0 ~ "^### " rb "\\." { on=1 }
    on && /^---$/ { exit }
    on && $0 ~ "^### " && $0 !~ "^### " rb "\\." { exit }
    on { print }
  ' "$1"
}

slice_surface() { # field-surface-audit.md, runbook id
  local f="$1" rb="$2"
  printf '# Read-surface reference for %s\n\n' "$rb"
  cat <<'PREAMBLE'
Extracted from `docs/design/field-surface-audit.md`, which was written
offline, before any run existed, from the vendored schema and the twelve
curated documents. It describes the tool, never this attempt.

Two things are deliberately absent. The audit's headline and its rulings
are not here, and `docs/runbooks/attemptability.md` is not here at all:
that document predicts, per runbook, how many steps will fail and which
one matters, which is a verdict rather than a reference.

PREAMBLE
  printf '## Per-kind surface: what the curated documents select\n\n'
  awk '/^## Per-kind surface/{on=1;next} on && /^## /{exit} on' "$f"
  printf '\n## Step rows naming %s\n\n' "$rb"
  printf '| Step | Blocker | Label |\n|---|---|---|\n'
  grep -E "^\| ${rb}\.[0-9]" "$f" || printf '| (none) | no row for this runbook | |\n'
  printf '\n'
  printf '%s\n' 'Labels: OURS/selection (field exists on the bound type, unselected),' \
                'OURS/binding (needs an unbound root such as `cloudResourcesV2`),' \
                'EXTERNAL (needs an input no security graph holds), WIZ (the API' \
                'genuinely cannot). The audit records zero steps labelled WIZ.'
}

# ---------------------------------------------------------------------
# packet
# ---------------------------------------------------------------------

cmd_packet() {
  local run_dir="" judge="" out=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --run-dir) run_dir="${2:?}"; shift 2 ;;
      --judge)   judge="${2:?}"; shift 2 ;;
      --out)     out="${2:?}"; shift 2 ;;
      *) die "packet: unknown argument: $1" ;;
    esac
  done
  [[ -n "$run_dir" ]] || die "packet: --run-dir is required"
  [[ -f "$run_dir/run.env" ]] || die "packet: no run at $run_dir" "$EX_STATE"
  [[ -s "$run_dir/runlog.jsonl" ]] || die "packet: $run_dir/runlog.jsonl is empty" "$EX_STATE"

  run_dir="$(cd "$run_dir" && pwd)"
  # shellcheck disable=SC1091
  source "$run_dir/run.env"
  local runbook="${RUNLOG_RUNBOOK:?}" run_id="${RUNLOG_RUN_ID:?}"

  [[ -n "$judge" ]] || judge="$(judge_for "$runbook")" \
    || die "packet: no judge assigned to runbook $runbook" "$EX_STATE"

  [[ -n "$out" ]] || out="$run_dir/judge/packet-${runbook}-${judge}"
  # A packet holds scrubbed tenant-derived output. It lives under the run
  # directory so it inherits that directory's `*` gitignore, the same
  # reasoning runlog.sh applies to data/. --out may retarget it inside
  # the run tree and nowhere else.
  case "$out" in
    "$run_dir"/*) ;;
    *) die "packet: --out must resolve under $run_dir" "$EX_STATE" ;;
  esac
  [[ -e "$out" ]] && die "packet: $out already exists" "$EX_STATE"
  mkdir -p "$out/data"

  # 1. the brief, sliced to this judge alone.
  [[ -f "$DOC_JUDGES" ]] || die "packet: missing $DOC_JUDGES" "$EX_STATE"
  slice_anchor "$DOC_JUDGES" "brief:$judge" > "$out/brief.md"
  [[ -s "$out/brief.md" ]] || die "packet: no brief anchor for judge '$judge'" "$EX_STATE"
  # A brief that carries another judge's anchor would hand this judge the
  # whole roster, which is the convergence hazard the roster is sized
  # against.
  if grep -q '<!-- brief:' "$out/brief.md"; then
    die "packet: brief slice for '$judge' contains another brief anchor" "$EX_STATE"
  fi

  # 2. the runbook, from the executor's copy. Never the provenance file.
  slice_runbook "$DOC_CATALOGUE" "$runbook" > "$out/runbook.md"
  [[ -s "$out/runbook.md" ]] || die "packet: runbook $runbook not found in catalogue" "$EX_STATE"

  # 3. the procedure and the verdict schema.
  slice_anchor "$DOC_JUDGES" "packet:instructions" > "$out/INSTRUCTIONS.md"
  [[ -s "$out/INSTRUCTIONS.md" ]] || die "packet: instructions anchor missing" "$EX_STATE"

  # 4. read-surface reference for this runbook.
  slice_surface "$DOC_SURFACE" "$runbook" > "$out/surface.md"

  # 5. the call record, allowlist-projected.
  project_runlog "$run_dir/runlog.jsonl" > "$out/calls.jsonl"

  # 6. artifacts, only those a permitted stave_call names.
  local copied=0 rel src
  while IFS= read -r rel; do
    [[ -n "$rel" && "$rel" != "null" ]] || continue
    src="$run_dir/$rel"
    [[ -f "$src" ]] || continue
    cp "$src" "$out/data/$(basename "$rel")"
    copied=$((copied + 1))
  done < <(jq -r 'select(.type=="stave_call") | .output_path // empty' "$out/calls.jsonl" | sort -u)

  # 7. manifest. Per-type EXCLUDED counts deliberately do not land here:
  # "three friction entries were removed" is itself a signal about how
  # the run went. The constant list of excluded types does land, so the
  # judge knows what it is not being shown.
  local files_json
  files_json="$(
    cd "$out"
    find . -type f ! -name MANIFEST.json | sort | while IFS= read -r f; do
      jq -n --arg p "${f#./}" --arg h "$(sha256_file "$f")" \
            --argjson b "$(wc -c < "$f" | tr -d ' ')" \
            '{path:$p, sha256:$h, bytes:$b}'
    done | jq -sc .
  )"
  jq -n \
    --argjson sv "$JUDGE_SCHEMA_VERSION" \
    --arg created "$(utc_now)" \
    --arg runbook "$runbook" --arg judge "$judge" \
    --arg run_id "$run_id" \
    --arg commit "$(cd "$REPO_ROOT" && git rev-parse --short HEAD 2>/dev/null || echo unknown)" \
    --argjson files "$files_json" \
    --argjson excluded "$EXCLUDED_TYPES" \
    --argjson entries "$(wc -l < "$run_dir/runlog.jsonl" | tr -d ' ')" \
    --argjson kept "$(wc -l < "$out/calls.jsonl" | tr -d ' ')" \
    --argjson artifacts "$copied" '
    {schema_version:$sv, created:$created, runbook:$runbook, judge:$judge,
     run_id:$run_id, repo_commit:$commit,
     runlog_entries_total:$entries, runlog_entries_shown:$kept,
     artifacts:$artifacts,
     excluded_entry_types:$excluded,
     files:$files}
  ' > "$out/MANIFEST.json"

  # Per-type counts go to stderr, for whoever is assembling, not to the
  # packet the judge reads.
  {
    printf 'packet: %s judged by %s\n' "$runbook" "$judge"
    printf '  %s\n' "$out"
    printf '  runlog entries %s, shown %s, artifacts %s\n' \
      "$(wc -l < "$run_dir/runlog.jsonl" | tr -d ' ')" \
      "$(wc -l < "$out/calls.jsonl" | tr -d ' ')" "$copied"
    printf '  withheld by type:\n'
    jq -r --argjson ex "$EXCLUDED_TYPES" \
      'select([.type] | inside($ex)) | .type' "$run_dir/runlog.jsonl" \
      | sort | uniq -c | sed 's/^/    /' || true
  } >&2

  printf '%s\n' "$out"
}

# ---------------------------------------------------------------------
# verdict
# ---------------------------------------------------------------------

VERDICT_RULES='
def outcomes: ["satisfied","partially_satisfied","not_satisfied"];
def causes: ["executor_shortfall","tool_cannot","gated","external_input_absent","unresolved"];
def remedies: ["selection","binding","capability","vendor"];
def err($m): {ok:false, msg:$m};

def check_step($s):
  if ($s.step | type) != "number" then err("step: `step` must be a number")
  elif ([$s.outcome] | inside(outcomes) | not) then err("step \($s.step): outcome must be one of \(outcomes)")
  elif $s.outcome != "satisfied" and ([$s.cause] | inside(causes) | not)
    then err("step \($s.step): cause is required unless outcome is satisfied")
  elif $s.cause == "tool_cannot" and ([$s.remedy] | inside(remedies) | not)
    then err("step \($s.step): tool_cannot requires remedy one of \(remedies)")
  elif $s.cause == "tool_cannot" and (($s.missing // "") | length) < 3
    then err("step \($s.step): tool_cannot requires `missing`, naming the field, root or argument that would close it")
  elif $s.cause == "executor_shortfall" and (($s.alternative // "") | length) < 3
    then err("step \($s.step): executor_shortfall requires `alternative`, naming what the executor could have done with the surface as it stands")
  elif (($s.reasoning // "") | length) < 3 then err("step \($s.step): reasoning is required")
  else {ok:true} end;

def validate:
  if .schema_version != 1 then err("schema_version must be 1")
  elif (.runbook // "") == "" then err("runbook is required")
  elif (.judge // "") == "" then err("judge is required")
  elif .authority != "judge" then err("authority must be the literal \"judge\"")
  elif ([.overall.outcome] | inside(outcomes) | not) then err("overall.outcome must be one of \(outcomes)")
  elif .overall.outcome != "satisfied" and ([.overall.cause] | inside(causes + ["mixed"]) | not)
    then err("overall.cause is required unless overall.outcome is satisfied")
  elif ((.steps // []) | length) == 0 then err("steps must be a non-empty array")
  elif ((.fitness.monthly_review // "") == "" or (.fitness.auditor // "") == ""
        or (.fitness.spoke_handoff // "") == "")
    then err("fitness needs monthly_review, auditor and spoke_handoff")
  elif ((.handwork // []) | type) != "array" then err("handwork must be an array")
  else ([.steps[] | check_step(.)] | map(select(.ok | not)) | if length > 0 then .[0] else {ok:true} end)
  end;
'

cmd_verdict() {
  local packet="" file=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --packet) packet="${2:?}"; shift 2 ;;
      --file)   file="${2:?}"; shift 2 ;;
      *) die "verdict: unknown argument: $1" ;;
    esac
  done
  [[ -n "$packet" ]] || die "verdict: --packet is required"
  [[ -f "$packet/MANIFEST.json" ]] || die "verdict: no packet at $packet" "$EX_STATE"
  [[ -n "$file" ]] || die "verdict: --file is required (use - for stdin)"
  # Write-once, for the same reason a coach CLEAR is single use: a
  # verdict that can be replaced after the divergence is computed is not
  # a measurement.
  [[ -e "$packet/verdict.json" ]] && die "verdict: $packet/verdict.json already exists" "$EX_STATE"

  local raw
  if [[ "$file" == "-" ]]; then raw="$(cat)"; else raw="$(cat "$file")"; fi
  printf '%s' "$raw" | jq -e . >/dev/null 2>&1 || die "verdict: not valid JSON" "$EX_SCHEMA"

  local check
  check="$(printf '%s' "$raw" | jq -r "$VERDICT_RULES"' validate | if .ok then "ok" else .msg end')"
  [[ "$check" == "ok" ]] || die "verdict: $check" "$EX_SCHEMA"

  local rb judge
  rb="$(jq -r .runbook <<<"$raw")"
  judge="$(jq -r .judge <<<"$raw")"
  local want_rb want_judge
  want_rb="$(jq -r .runbook "$packet/MANIFEST.json")"
  want_judge="$(jq -r .judge "$packet/MANIFEST.json")"
  [[ "$rb" == "$want_rb" ]] || die "verdict: runbook $rb does not match packet $want_rb" "$EX_SCHEMA"
  [[ "$judge" == "$want_judge" ]] || die "verdict: judge $judge does not match packet $want_judge" "$EX_SCHEMA"

  # Pattern-scrub every free-text field, then stamp the packet hash so a
  # verdict is bound to the packet it was formed from.
  local scrubbed
  scrubbed="$(printf '%s' "$raw" | "$SCRUB" --text)"
  printf '%s' "$scrubbed" | jq -e . >/dev/null 2>&1 \
    || die "verdict: the scrubber altered the JSON structure; report this" "$EX_SCHEMA"

  printf '%s' "$scrubbed" | jq \
    --arg ph "$(sha256_file "$packet/MANIFEST.json")" \
    --arg rec "$(utc_now)" \
    '. + {packet_sha256:$ph, recorded:$rec}' > "$packet/verdict.json"

  printf 'verdict recorded: %s %s -> %s\n' "$rb" "$judge" \
    "$(jq -r .overall.outcome "$packet/verdict.json")"
}

# ---------------------------------------------------------------------
# diverge
# ---------------------------------------------------------------------
#
# Computed once, over every verdict that exists, and never per runbook as
# the verdicts land. A statistic recomputed after each new data point is
# a statistic somebody can stop at.

cmd_diverge() {
  local root="$REPO_ROOT/runs" as_json=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --runs-root) root="${2:?}"; shift 2 ;;
      --json) as_json=1; shift ;;
      *) die "diverge: unknown argument: $1" ;;
    esac
  done
  [[ -d "$root" ]] || die "diverge: no runs root at $root" "$EX_STATE"

  DIVERGE_PAIRS="$(mktemp)"
  trap 'rm -f "$DIVERGE_PAIRS"' EXIT
  local pairs="$DIVERGE_PAIRS"
  : > "$pairs"

  local v rd
  while IFS= read -r v; do
    rd="$(cd "$(dirname "$v")/../.." && pwd)"
    [[ -f "$rd/runlog.jsonl" ]] || continue
    # Executor side: one step_result per step. Last write wins, which is
    # what runlog.sh's own append semantics produce.
    jq -c 'select(.type=="step_result") | {step, executor: .outcome}' "$rd/runlog.jsonl" \
      | jq -sc 'group_by(.step) | map(.[-1]) | INDEX(.step|tostring)' > "$rd/.exec.tmp"
    jq -c --slurpfile ex "$rd/.exec.tmp" '
      . as $vd
      | ($ex[0] // {}) as $e
      | .steps[]
      | {runbook: $vd.runbook, judge: $vd.judge, step: .step,
         judge_outcome: .outcome, cause: (.cause // null), remedy: (.remedy // null),
         executor_outcome: ($e[(.step|tostring)].executor // null)}
    ' "$v" >> "$pairs"
    rm -f "$rd/.exec.tmp"
  done < <(find "$root" -type f -path '*/judge/*/verdict.json' | sort)

  [[ -s "$pairs" ]] || die "diverge: no verdicts found under $root" "$EX_STATE"

  local report
  report="$(jq -sc '
    def ord($o): {"not_satisfied":0,"partially_satisfied":1,"satisfied":2,
                  "unmet":0,"partial":1,"met":2}[$o];
    def cls: .runbook | .[0:1];
    map(. + {j: ord(.judge_outcome), e: ord(.executor_outcome)}) as $all
    | ($all | map(select(.e != null))) as $matched
    | {
        runbooks_judged: ($all | map(.runbook) | unique | length),
        judge_steps: ($all | length),
        comparable_steps: ($matched | length),
        judge_only_steps: ($all | length) - ($matched | length),
        agree: ($matched | map(select(.j == .e)) | length),
        judge_harsher: ($matched | map(select(.j < .e)) | length),
        judge_softer: ($matched | map(select(.j > .e)) | length),
        mean_signed_delta: (if ($matched|length) > 0
          then (($matched | map(.j - .e) | add) / ($matched | length) * 100 | round / 100)
          else null end),
        by_class: ($matched | group_by(cls) | map({
          class: (.[0] | cls), pairs: length,
          agree: (map(select(.j == .e)) | length),
          harsher: (map(select(.j < .e)) | length),
          softer: (map(select(.j > .e)) | length),
          mean_delta: ((map(.j - .e) | add) / length * 100 | round / 100)})),
        by_judge: ($matched | group_by(.judge) | map({
          judge: .[0].judge, pairs: length,
          agree: (map(select(.j == .e)) | length),
          harsher: (map(select(.j < .e)) | length),
          softer: (map(select(.j > .e)) | length),
          mean_delta: ((map(.j - .e) | add) / length * 100 | round / 100)})),
        causes: ($all | map(select(.judge_outcome != "satisfied"))
                      | group_by(.cause) | map({cause: .[0].cause, n: length})),
        tool_cannot_remedies: ($all | map(select(.cause == "tool_cannot"))
                      | group_by(.remedy) | map({remedy: .[0].remedy, n: length})),
        verb_evidence_steps: ($all | map(select(.cause == "tool_cannot" and .remedy == "capability")) | length)
      }' "$pairs")"

  if [[ "$as_json" -eq 1 ]]; then printf '%s\n' "$report" | jq .; return 0; fi

  jq -r '
    "divergence: judge verdicts x executor step_results",
    "",
    "  runbooks judged        \(.runbooks_judged)",
    "  judge step verdicts    \(.judge_steps)",
    "  comparable pairs       \(.comparable_steps)   (executor recorded a step_result)",
    "  judge-only steps       \(.judge_only_steps)   (no executor step_result to compare)",
    "",
    "  agree                  \(.agree)",
    "  judge harsher          \(.judge_harsher)",
    "  judge softer           \(.judge_softer)",
    "  mean signed delta      \(.mean_signed_delta)   (judge minus executor; negative = judge harsher)",
    "",
    "  by class",
    (.by_class[] | "    \(.class)  pairs \(.pairs)  agree \(.agree)  harsher \(.harsher)  softer \(.softer)  mean \(.mean_delta)"),
    "",
    "  by judge",
    (.by_judge[] | "    \(.judge | . + "        " | .[0:8])  pairs \(.pairs)  agree \(.agree)  harsher \(.harsher)  softer \(.softer)  mean \(.mean_delta)"),
    "",
    "  cause, over steps the judge did not mark satisfied",
    (.causes[] | "    \(.cause // "none")  \(.n)"),
    "",
    "  tool_cannot remedy",
    (.tool_cannot_remedies[] | "    \(.remedy // "none")  \(.n)"),
    "",
    "  verb evidence steps    \(.verb_evidence_steps)   <- tool_cannot with remedy=capability, the only rows that are evidence for a new verb"
  ' <<<"$report"
}

# ---------------------------------------------------------------------
# selftest
# ---------------------------------------------------------------------
#
# Synthetic values only. No tenant, no credentials, no network, and no
# stave invocation: the run directory is hand-built.

selftest() {
  local fail=0
  JUDGE_SELFTEST_ROOT="$(mktemp -d)"
  trap 'rm -rf "$JUDGE_SELFTEST_ROOT"' EXIT
  local root="$JUDGE_SELFTEST_ROOT"

  ok_() { printf 'ok   %-52s\n' "$1"; }
  bad_() { printf 'FAIL %-52s %s\n' "$1" "${2:-}" >&2; fail=1; }

  local rd="$root/run"
  mkdir -p "$rd/data" "$rd/audit"
  printf '*\n' > "$rd/.gitignore"
  cat > "$rd/run.env" <<'ENV'
RUNLOG_RUN_ID='0123456789abcdef'
RUNLOG_SESSION_ID='selftest-A1-0001'
RUNLOG_RUNBOOK='A1'
RUNLOG_AUDIT_DIR='audit'
ENV

  # Needles planted in every excluded field. If any reaches the packet,
  # the allowlist has a hole.
  local N_RESULT="NEEDLE-STEPRESULT-GAP"
  local N_FRICTION="NEEDLE-FRICTION-WHAT"
  local N_DEADEND="NEEDLE-DEADEND-WHY"
  local N_OOB="NEEDLE-OOB-PURPOSE"
  local N_INTENT="NEEDLE-STEPSTART-INTENT"
  local N_RULING="NEEDLE-HUMAN-RULING"
  local N_TRACE="NEEDLE-TRACE-ID"

  cat > "$rd/runlog.jsonl" <<EOF
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":null,"seq":1,"ts":"2026-08-07T00:00:00.000Z","type":"run_start","repo_commit":"deadbee","audit_dir":"audit","run_dir":"run"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":1,"seq":2,"ts":"2026-08-07T00:00:01.000Z","type":"step_start","intent":"$N_INTENT","criterion":"one record per open issue"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":1,"seq":3,"ts":"2026-08-07T00:00:02.000Z","type":"coach_verdict","verdict":"CLEAR","command":"stave list issue --limit 3","command_sha256":"aa","reason":"bounded read.","doubt":null,"to_resolve":null,"coach_block_sha256":"bb"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":1,"seq":4,"ts":"2026-08-07T00:00:03.000Z","type":"stave_call","command":"stave list issue --limit 3","command_sha256":"aa","mode":"source","verdict_ref":"bb","exit_code":0,"scrub_exit":0,"duration_ms":113,"output_path":"data/issues.jsonl","output_lines":2,"output_bytes":200,"trace_ids":["$N_TRACE"],"operations":["list_issues"],"results":["ok"],"audit_lines":1,"stderr_excerpt":null}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":1,"seq":5,"ts":"2026-08-07T00:00:04.000Z","type":"out_of_band","tool":"jq","purpose":"$N_OOB","text":"jq -c .","survives_fix":false,"deleted_by":"gs23","survives_fix_reason":"server groups"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":1,"seq":6,"ts":"2026-08-07T00:00:05.000Z","type":"dead_end","command":"stave list issue --limit 3","description":null,"why":"$N_DEADEND"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":1,"seq":7,"ts":"2026-08-07T00:00:06.000Z","type":"friction","what":"$N_FRICTION","cost":null,"category":"scrub_refused"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":1,"seq":8,"ts":"2026-08-07T00:00:07.000Z","type":"step_result","outcome":"met","gap":"$N_RESULT","authority":"executor"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":2,"seq":9,"ts":"2026-08-07T00:00:08.000Z","type":"halt","command":"stave list issue","doubt":"d","to_resolve":"r","disposition":"pending"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":2,"seq":10,"ts":"2026-08-07T00:00:09.000Z","type":"halt","disposition":"skip","human_ruling":"$N_RULING","attested_by":"executor"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":2,"seq":11,"ts":"2026-08-07T00:00:10.000Z","type":"step_result","outcome":"partial","gap":"$N_RESULT two","authority":"executor"}
{"schema_version":1,"run_id":"0123456789abcdef","session_id":"selftest-A1-0001","runbook":"A1","step":null,"seq":12,"ts":"2026-08-07T00:00:11.000Z","type":"run_end","reason":"finished","note":"$N_RESULT three"}
EOF
  printf '{"_kind":"issue","severity":"CRITICAL"}\n{"_kind":"issue","severity":"HIGH"}\n' \
    > "$rd/data/issues.jsonl"
  printf '{"trace_id":"%s"}\n' "$N_TRACE" > "$rd/audit/day.jsonl"
  printf 'NEEDLE-UNREFERENCED\n' > "$rd/data/orphan.jsonl"

  local pkt rc=0
  pkt="$("$0" packet --run-dir "$rd" 2>/dev/null)" || rc=$?
  if [[ "$rc" -eq 0 && -f "$pkt/MANIFEST.json" ]]; then
    ok_ "packet: assembles from a run directory"
  else
    bad_ "packet: assembles from a run directory" "rc=$rc"; printf '%s\n' "$fail"; return 1
  fi

  # 1. Every excluded entry type is absent.
  local t missing_ok=1
  for t in step_result friction dead_end out_of_band; do
    if grep -q "\"type\":\"$t\"" "$pkt/calls.jsonl"; then
      bad_ "allowlist: entry type $t dropped"; missing_ok=0
    fi
  done
  [[ "$missing_ok" -eq 1 ]] && ok_ "allowlist: all four excluded types dropped"

  # 2. No planted needle from any excluded field or type reaches the packet.
  local leaked=0 needle
  for needle in "$N_RESULT" "$N_FRICTION" "$N_DEADEND" "$N_OOB" "$N_INTENT" \
                "$N_RULING" "$N_TRACE" "NEEDLE-UNREFERENCED"; do
    if grep -rqF -- "$needle" "$pkt" 2>/dev/null; then
      bad_ "allowlist: '$needle' reached the packet"; leaked=1
    fi
  done
  [[ "$leaked" -eq 0 ]] && ok_ "allowlist: no withheld value reached the packet"

  # 3. Positive control: the packet is not empty of run facts.
  if grep -q '"operations":\["list_issues"\]' "$pkt/calls.jsonl" \
     && grep -q 'CRITICAL' "$pkt/data/issues.jsonl"; then
    ok_ "allowlist: permitted facts and artifacts survived"
  else
    bad_ "allowlist: permitted facts and artifacts survived"
  fi

  # 4. Only artifacts a permitted stave_call names are copied.
  [[ -f "$pkt/data/issues.jsonl" && ! -f "$pkt/data/orphan.jsonl" ]] \
    && ok_ "packet: copies only artifacts a stave_call names" \
    || bad_ "packet: copies only artifacts a stave_call names"

  # 5. The audit trail is never copied.
  [[ ! -e "$pkt/audit" ]] && ok_ "packet: audit trail is not copied" \
    || bad_ "packet: audit trail is not copied"

  # 6. The brief is sliced to one judge.
  if [[ -s "$pkt/brief.md" ]] && ! grep -q '<!-- brief:' "$pkt/brief.md" \
     && grep -qi 'priya' "$pkt/brief.md"; then
    ok_ "packet: brief sliced to the assigned judge alone"
  else
    bad_ "packet: brief sliced to the assigned judge alone"
  fi

  # 7. The runbook slice is this runbook and stops before the next.
  if grep -q '^### A1\.' "$pkt/runbook.md" && ! grep -q '^### A2\.' "$pkt/runbook.md" \
     && grep -q 'Success criterion' "$pkt/runbook.md"; then
    ok_ "packet: runbook slice carries A1 and its criterion only"
  else
    bad_ "packet: runbook slice carries A1 and its criterion only"
  fi

  # 8. attemptability.md never reaches a packet: it predicts the verdict.
  if ! grep -rqF 'attemptability' "$pkt/surface.md" \
     || ! grep -rqF 'not here at all' "$pkt/surface.md"; then
    bad_ "packet: surface reference states the attemptability exclusion"
  else
    ok_ "packet: surface reference states the attemptability exclusion"
  fi
  if grep -rqE 'Blocked by|Reachable steps' "$pkt/surface.md"; then
    bad_ "packet: attemptability prediction table absent"
  else
    ok_ "packet: attemptability prediction table absent"
  fi

  # 9. --out outside the run tree is refused.
  rc=0; "$0" packet --run-dir "$rd" --out "$root/elsewhere" >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_STATE" && ! -e "$root/elsewhere" ]] \
    && ok_ "packet: --out outside the run tree refused" \
    || bad_ "packet: --out outside the run tree refused" "rc=$rc"

  # 10. The script's assignment table agrees with the design document.
  local doc_tbl script_tbl
  script_tbl="$("$0" assignments)"
  doc_tbl="$(slice_anchor "$DOC_JUDGES" "assignment:table" \
    | awk -F'|' 'NF>3 && $2 ~ /^ *[ABC][0-9]+ *$/ {gsub(/ /,"",$2); gsub(/ /,"",$4); print $2"\t"$4}')"
  if [[ -n "$doc_tbl" && "$doc_tbl" == "$script_tbl" ]]; then
    ok_ "assignment: script table matches the design document"
  else
    bad_ "assignment: script table matches the design document" \
      "$(diff <(printf '%s\n' "$script_tbl") <(printf '%s\n' "$doc_tbl") | head -6 | tr '\n' ' ')"
  fi

  # 11. Verdict validation refuses what it must.
  vjson() { # outcome, cause, remedy, missing, alternative
    jq -n --arg o "$1" --arg c "$2" --arg r "$3" --arg m "$4" --arg a "$5" '
      {schema_version:1, runbook:"A1", judge:"priya", authority:"judge",
       overall:{outcome:$o, cause:(if $c=="" then null else $c end), reasoning:"synthetic"},
       steps:[{step:1, outcome:$o, cause:(if $c=="" then null else $c end),
               remedy:(if $r=="" then null else $r end),
               missing:(if $m=="" then null else $m end),
               alternative:(if $a=="" then null else $a end),
               reasoning:"synthetic", evidence:"data/issues.jsonl"}],
       handwork:["synthetic"],
       fitness:{monthly_review:"no", auditor:"no", spoke_handoff:"no"}}'
  }
  rc=0; vjson not_satisfied "" "" "" "" | "$0" verdict --packet "$pkt" --file - >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_SCHEMA" ]] && ok_ "verdict: not_satisfied without a cause refused" \
    || bad_ "verdict: not_satisfied without a cause refused" "rc=$rc"

  rc=0; vjson not_satisfied tool_cannot "" "" "" | "$0" verdict --packet "$pkt" --file - >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_SCHEMA" ]] && ok_ "verdict: tool_cannot without a remedy refused" \
    || bad_ "verdict: tool_cannot without a remedy refused" "rc=$rc"

  rc=0; vjson not_satisfied tool_cannot capability "" "" | "$0" verdict --packet "$pkt" --file - >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_SCHEMA" ]] && ok_ "verdict: tool_cannot without naming what is missing refused" \
    || bad_ "verdict: tool_cannot without naming what is missing refused" "rc=$rc"

  rc=0; vjson not_satisfied executor_shortfall "" "" "" | "$0" verdict --packet "$pkt" --file - >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_SCHEMA" ]] && ok_ "verdict: shortfall without an alternative refused" \
    || bad_ "verdict: shortfall without an alternative refused" "rc=$rc"

  rc=0
  vjson partially_satisfied tool_cannot selection "Issue.assignee is unselected" "" \
    | "$0" verdict --packet "$pkt" --file - >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq 0 && -f "$pkt/verdict.json" ]] \
    && ok_ "verdict: a well-formed verdict is recorded" \
    || bad_ "verdict: a well-formed verdict is recorded" "rc=$rc"

  # 12. Write once.
  rc=0
  vjson satisfied "" "" "" "" | "$0" verdict --packet "$pkt" --file - >/dev/null 2>&1 || rc=$?
  [[ "$rc" -eq "$EX_STATE" ]] && ok_ "verdict: write-once" \
    || bad_ "verdict: write-once" "rc=$rc"

  # 13. The verdict is bound to the packet it was formed from.
  jq -e --arg h "$(sha256_file "$pkt/MANIFEST.json")" \
     '.packet_sha256 == $h and .authority == "judge"' "$pkt/verdict.json" >/dev/null \
    && ok_ "verdict: bound to the packet hash" \
    || bad_ "verdict: bound to the packet hash"

  # 14. Divergence arithmetic. Executor said `met` at step 1, judge said
  # partially_satisfied: ordinals 2 and 1, so delta -1, judge harsher.
  local dv
  dv="$("$0" diverge --runs-root "$root" --json)"
  if [[ "$(jq -r '.comparable_steps' <<<"$dv")" == "1" \
     && "$(jq -r '.judge_harsher' <<<"$dv")" == "1" \
     && "$(jq -r '.mean_signed_delta' <<<"$dv")" == "-1" \
     && "$(jq -r '.verb_evidence_steps' <<<"$dv")" == "0" ]]; then
    ok_ "diverge: signed delta, direction and verb-evidence count"
  else
    bad_ "diverge: signed delta, direction and verb-evidence count" \
      "$(jq -c '{comparable_steps,judge_harsher,mean_signed_delta,verb_evidence_steps}' <<<"$dv")"
  fi

  printf '\n'
  if [[ "$fail" -eq 0 ]]; then printf 'judge.sh selftest: all checks passed\n'; else
    printf 'judge.sh selftest: FAILURES\n' >&2; fi
  return "$fail"
}

# ---------------------------------------------------------------------
# main
# ---------------------------------------------------------------------

main() {
  need jq
  local sub="${1:-}"
  [[ $# -gt 0 ]] && shift || true
  case "$sub" in
    packet)      cmd_packet "$@" ;;
    verdict)     cmd_verdict "$@" ;;
    diverge)     cmd_diverge "$@" ;;
    assignments) cmd_assignments "$@" ;;
    selftest)    selftest ;;
    ""|-h|--help|help)
      sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      ;;
    *) die "unknown subcommand: $sub" ;;
  esac
}

main "$@"
