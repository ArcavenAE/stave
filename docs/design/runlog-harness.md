# The run-log harness

bd `aae-orc-e4jo.9`. Written 2026-08-07. Implementation:
`scripts/runlog.sh`. Worked example: `examples/runlog/`.

The audit trail records what stave did. It cannot record what a step was
trying to achieve, what happened outside stave, whether the step met the
runbook's own criterion, or which calls were dead ends. Those four are
the mining signal, and this harness is where they get written down.

It emits `runlog.jsonl` beside the audit trail, correlated by the same
`STAVE_SESSION_ID` and joinable to the audit lines on `session_id` and
`trace_id`.

Two properties hold by construction rather than by anyone remembering:
tenant output is scrubbed on the way out, and no invocation runs without
a recorded CLEAR verdict for that exact invocation. Both have limits.
Those limits are stated below rather than in a footnote, because a
control whose gaps are not written down gets trusted past them.

---

## Why bash

Three reasons, in order of weight.

**It has to compose with the scrubber, which is bash.** Scrub-by-
construction means `scripts/scrub.sh` sits inside the harness's read
path. A Rust harness would shell out to it anyway, and would then own a
second implementation of the argument handling, the tier logic, and the
refusal codes.

**The run is a sequence of turns, not a program.** Between one
invocation and the next, an agent consults a Claude subagent and waits.
The harness is invoked once per step, holds its state on disk, and
exits. That is a shell shape. A long-running Rust process would spend
its design budget on a resumable state machine that a directory already
gives for free.

**It must not enter the shipped binary's build graph.** This is a lab
instrument for one exercise. A crate in the workspace would be built by
`cargo build`, linted by `just check`, and would look like a product
surface. `scripts/` is where this repo already keeps instruments that
are not the tool.

The cost is real: bash has no type checking, and the selftest carries
more weight than it would in Rust. That is why the selftest is wired
into `just hygiene` and `just ci` beside the scrubber's own.

---

## Run layout

```
runs/<runbook>-<utc-stamp>/
  run.env             run id, session id, runbook, audit dir
  runlog.jsonl        the record (this document's subject)
  .gitignore          contains `*`
  audit/              stave's own audit trail for this run, UNSCRUBBED
  data/               scrubbed output, one file per invocation
  verdicts/           pending single-use CLEAR records, by command hash
  verdicts/consumed/  spent verdicts, kept for review
  state/              seq counters, current step
  tmp/                scratch, cleared per invocation
  HALTED              present only while the run is halted
```

`runs/` is gitignored at the repo root, and `init` writes a `*`
gitignore inside every run directory. Both, because the audit trail in
`audit/` is stave's own output: the scrubber never sees it, and it
carries GraphQL variables, cursors, hostname, and username.

`init` prints the one export the rest of the run needs:

```sh
export STAVE_RUNLOG_DIR='<run dir>'
```

---

## The entry schema

One JSON object per line. `schema_version` is 1.

### Common header, on every entry

| Field | Type | Meaning |
|---|---|---|
| `schema_version` | int | 1. Bump on incompatible change. |
| `run_id` | string | 16 hex chars, unique per run. |
| `session_id` | string | The value exported as `STAVE_SESSION_ID`. **Join key to the audit trail.** |
| `runbook` | string | `A1`, `B7`, and so on. |
| `step` | int or null | Runbook step number. Null before the first `step`. |
| `seq` | int | Monotonic within a run, contiguous, no gaps. Orders everything. |
| `ts` | string | RFC 3339 UTC, millisecond precision. |
| `type` | string | One of the eleven below. |

### `run_start`, `run_end`

Bookkeeping. `run_start` carries `repo_commit`, `audit_dir`, `run_dir`;
`run_end` carries `reason` and an optional `note`. Not in the ticket's
list, added because the reconcile join and the judge both need to know
where a run began and whether it ended.

### `step_start`

| Field | Meaning |
|---|---|
| `intent` | What the step is trying to achieve, **in the operator's terms**, not the tool's. |
| `criterion` | The runbook's own success criterion for this step, or null. |

`criterion` is what makes `step_result` checkable rather than an
opinion. It is optional because not every runbook step states one.

### `stave_call`

| Field | Meaning |
|---|---|
| `command` | The canonical invocation text. Identical to what the coach reviewed. |
| `command_sha256` | Hash of that text. Joins to the `coach_verdict` entry. |
| `mode` | `source` (reached the tenant; scrubbed), `stream` (read a scrubbed file, emitted JSONL; scrubbed again) or `render` (read a scrubbed file, emitted a rendered document; the one unscrubbed path). |
| `verdict_ref` | `coach_block_sha256` of the verdict that licensed this call. |
| `exit_code` | The invocation's exit status. |
| `scrub_exit` | The scrubber's exit status. Nonzero means the output was discarded. |
| `duration_ms` | Wall clock around the whole pipe. |
| `output_path` | Path relative to the run dir, or null when nothing was written. |
| `output_lines`, `output_bytes` | Size of the scrubbed output. |
| `trace_ids` | **Join key to the audit trail.** Distinct `trace_id` values this call produced. |
| `operations` | Distinct `operation.id` values seen, from the audit lines. |
| `results` | Distinct `result` enum values seen. |
| `audit_lines` | How many audit lines the call wrote. |
| `stderr_excerpt` | First 2000 bytes of stderr, pattern-scrubbed, or null. |

`trace_ids`, `operations`, and `results` are lifted back out of the
audit trail after the call returns. Only join keys and enums are lifted.
Variables, argv, and cursors stay in the audit file, because those are
the tenant-identifying half.

### `coach_verdict`

One per proposed invocation, CLEAR or HALT.

| Field | Meaning |
|---|---|
| `verdict` | `CLEAR` or `HALT`. Parsed from the coach's block, never from a flag. |
| `command`, `command_sha256` | The invocation reviewed. |
| `reason` | The coach's REASON line. |
| `doubt`, `to_resolve` | HALT only, verbatim from the coach. Null on CLEAR. |
| `coach_block_sha256` | Hash of the coach's raw output block. |

### `out_of_band`

Work done outside stave. This is the negative space where a verb might
belong, so `purpose` carries the weight and the command text is
incidental.

| Field | Meaning |
|---|---|
| `tool` | `jq`, `shell`, `manual`, `other`. |
| `purpose` | What the stage does to the data, not how. One phrase. |
| `text` | The command, if there was one. Pattern-scrubbed. |
| `survives_fix` | **Required.** Would this stage still be necessary if `aae-orc-rsh6`, `j1xi`, `qijl`, and `gs23` had landed? |
| `deleted_by` | Which of those four tickets removes it. Required when `survives_fix` is false. |
| `survives_fix_reason` | Why, in one sentence. |

`survives_fix` is mandatory and the harness refuses the entry without
it. It is the one addition beyond the ticket's list, and it exists
because the ticket's own motivating example turned out to be a false
positive: "list_issues was called 40 times because there is no
server-side filter" is document debt, not verb demand. `issuesV2`
accepts `filterBy: IssueFilters` today and stave's documents declare
only `$first` and `$after`. See `docs/design/field-surface-audit.md` and
`docs/runbooks/paper-pipelines.md`, where the same tagging deletes 16 of
106 paper glue stages, 10 of them in class A alone. Without the field
the runlog reproduces that false positive in executed form and the
mining stage counts our own backlog as demand for new verbs.

Requiring a ticket id when `survives_fix` is false is deliberate
friction. "It would still be needed" is cheap to assert; naming which of
four tickets deletes the stage is not.

### `dead_end`

A call made and discarded. `command` (canonical text) or `description`,
plus `why`. The `why` is the whole value: a dead end with no reason is
just a call that already appears in the audit trail.

### `step_result`

| Field | Meaning |
|---|---|
| `outcome` | `met`, `partial`, `unmet`, in the runbook's terms. |
| `gap` | What is missing. Required for partial and unmet. |
| `authority` | Always `executor`. |

`authority` is fixed rather than settable because of the ticket's own
design note: this is the executor's view and it is explicitly not
authoritative. The judge (`aae-orc-e4jo.10`) rules independently and the
divergence between the two is itself a measurement.

### `friction`

`what`, optional `cost`, and `category`. The harness writes one itself
with `category: "scrub_refused"` whenever the scrubber refuses an
output, since that is friction the executor did not choose to record.

### `halt`

Two entries per halt. The first, written with the HALT verdict, carries
`command`, `doubt`, `to_resolve`, and `disposition: "pending"`. The
second, written by `resume`, carries `disposition`
(`skip` / `proceed-modified` / `stop`), `human_ruling`, and
`attested_by: "executor"` with a note that the harness cannot verify a
human made the ruling.

Two entries rather than one because the judge and the mining stage need
to see which steps were never attempted and why. Without them a runbook
that halted looks identical to one that failed.

---

## Joining to the audit trail

```
runlog.session_id      ==  audit.invocation.session_id
runlog.trace_ids[]     ==  audit.trace_id
runlog.command_sha256  ==  runlog.command_sha256   (stave_call to coach_verdict)
runlog.verdict_ref     ==  runlog.coach_block_sha256
```

`runlog.sh reconcile` runs the first two and reports four counts:
matched, audit-only, runlog-only, and the total on each side. It exits
nonzero when any audit line carries this run's session id with no
matching `stave_call`, because that means stave ran outside the harness:
no coach verdict, and output that was never scrubbed.

That is detection, not prevention, and it is the honest answer to
"what if the agent just runs `stave` in a bare shell call". The harness
cannot stop it. The audit trail is written by stave itself, so it
records the bypass whether or not the harness was involved, and
`reconcile` surfaces it at the end of the run.

The exercise that exists to justify a join verb needs a join to analyse
itself. That is worth recording as evidence rather than as irony.

---

## The gate and its limits

`.claude/rules/safety-coach-gate.md` says the gate belongs at the point
invocations are generated, per invocation, at run time, and that
reviewing the harness once when it was written is not the gate.

A bash script cannot invoke a Claude subagent, so the gate cannot
literally be a call. It is a **precondition** instead.

### The mechanism

1. `runlog.sh canon -- stave list issue --limit 3` renders the argv as
   one canonical line. Deterministic, so the text the coach reviews and
   the argv that runs are derived from the same array.
2. The driving agent hands that exact text to the `stave-safety-coach`
   subagent and gets back a block.
3. `runlog.sh verdict --coach-file - -- stave list issue --limit 3`
   parses the block. The `COMMAND:` line must equal the canonical text
   byte for byte. `VERDICT:` is read from the block, never from a flag.
   A CLEAR writes a single-use record keyed by the hash of the canonical
   text.
4. `runlog.sh exec -- stave list issue --limit 3` hashes the argv it is
   about to run, looks for a matching unspent record, and refuses with
   exit 4 if there is none. On success the record is consumed.

Execution is by argv, never through a shell. There is no `eval`, so
there is no path by which a reviewed string and an executed command can
differ, and no shell metacharacter surface to review.

Single use matters because the coach's own check 4 halts on "a repeat of
a bulk pull already performed this session". A verdict that could be
replayed would license exactly the thing the coach is watching for.

A HALT writes the verdict entry, a `halt` entry, and a `HALTED` file.
While that file exists `exec` refuses everything, including invocations
that already hold a CLEAR. Only `resume` with a recorded human ruling
clears it. This is the mechanical answer to "halt, then quietly
reformulate around it".

### What this does not do

**It cannot prove a subagent was consulted.** Nothing in a shell script
can. A driving agent could compose a coach block itself and feed it to
`verdict`.

What the mechanism changes is the character of the failure. Skipping the
coach stops being an omission and becomes a forgery: the agent must
write a `VERDICT:` line, a matching `COMMAND:` line, and a plausible
`REASON:` in the coach's format, knowing the block's hash lands in the
runlog. Omissions happen under time pressure. Forgeries are deliberate,
and this one is reviewable after the fact, because every verdict is in
`runlog.jsonl` and every spent record is in `verdicts/consumed/` with
its reason. A human or the judge can spot-check any of them against what
the coach would actually have said.

That is a weaker claim than "structurally impossible" and it is the true
one. The property that IS structural is narrower and worth stating
exactly:

> It is impossible for the harness to execute an invocation for which no
> verdict record with a matching command hash exists.

Closing the remaining gap needs the gate to live where the subagent can
be called, which is the driving agent's own loop. Two shapes could do it
and neither is available today: a hook that fires on the Bash tool and
calls the coach before any `stave` argv, or an MCP server holding the
coach behind a tool call the harness could make. Both belong to the
harness's environment rather than to the harness. Recorded here so the
next person does not mistake the precondition for the whole answer.

**It cannot verify the human ruling after a halt.** `resume --ruling`
records what the executor says the human decided, and the entry says so
in its own `note` field.

**It does not cover invocations that never reach the harness.** See
reconcile, above: detected, not prevented.

---

## Scrub by construction, and its limits

`exec` in source mode runs `<argv> | scripts/scrub.sh > data/<name>`.
There is no bypass flag and no `--raw`. The only pass-through is
`--catalog`, which is the scrubber's own operator-attested mode for
vendor-published control and framework names.

Consequences that hold without anyone remembering anything:

- Raw tenant bytes exist only inside that pipe. They are never written
  to a durable path by the harness.
- If the scrubber refuses a shape (exit 3), the output file is removed,
  a `friction` entry is written, and `exec` exits 5. Fail closed. The
  common case is a rendered markdown table, where the field names are
  already gone and the allowlist cannot be applied: scrub before emit,
  not after.
- Every downstream `jq` stage in a pipeline reads from `data/`, so the
  executor never holds an unscrubbed record at all.
- Any file argument in the argv must resolve under `data/`. A stream
  stage cannot pull in an auxiliary stream from outside the run and
  launder it through the harness.
- **The mode is decided by the VERB, never by a flag.** `--in` is
  accepted only for `filter`, `enrich`, and `emit`; any other verb with
  `--in` is a usage error. Of those three only `emit` skips the
  scrubber, because it renders and the field allowlist cannot classify a
  rendered table. `filter` and `enrich` emit JSONL and are scrubbed
  again, which is free: the scrubber is idempotent over already-scrubbed
  JSONL.

  This is a correction, and the original is worth keeping because it is
  the shape a bypass takes when nobody adds one. The first version keyed
  the mode on `--in` alone, which made `--in` a scrubber bypass by
  combination rather than by intent: `--in <scrubbed file> -- stave list
  issue` passes the gate (the coach reviews the canonical argv and never
  sees a harness flag), ignores the stdin it was handed, reaches the
  tenant, and writes the raw answer into `data/`. Measured against a stub
  on 2026-08-07: a person's name and an ARN survived intact. There was no
  `--raw` flag and no bypass switch anywhere in the script, and the
  bypass existed anyway, out of two features that were each correct
  alone. Selftests 6b/6c/6d hold it closed.

  The residual: `render` trusts `emit` to be a pure renderer of its
  stdin. That is a property of the CLI rather than of the harness. If
  `emit` ever fetches anything, this exemption has to go.
- Free text the executor writes (intents, purposes, reasons, rulings)
  goes through the scrubber's pattern tier.

### Limits

**The pattern tier cannot catch a name.** Free-text fields are backed by
patterns only, which catch emails, GUIDs, ARNs, OCIDs, IPs, account ids,
and the local literals. A person's name, a bucket name, or a project
slug has no shape. The rule stands that intents and purposes describe
work in general terms; the scrub pass is a backstop, not a licence.

**The audit trail is out of reach by design.** stave writes `audit/`
itself and the scrubber never sees it. It is tenant-identifying, which
is why `runs/` is gitignored twice over.

**The scrubber buffers stdin to a `mktemp` file.** Raw tenant bytes
touch the filesystem transiently inside `scrub.sh`, in the system temp
directory, removed by a trap on return. Outside the run's artifact tree,
but not nothing. Recorded rather than fixed, because the fix belongs to
`scrub.sh` and changing a load-bearing control was not this ticket's
scope.

**Join keys do not survive scrubbing.** `id` is not on the field
allowlist, so every record id becomes the same literal
`<redacted:id>`. Any runbook stage that joins two streams on a record id
is not executable through this harness. Class A's owner attribution and
every class B join hit this.

This is a real limit on what the commissioning run can execute, and it
is not a reason to add a bypass. The fix, if the gate opens, is a
pseudonymising mode in the scrubber: HMAC the value with a per-run key
so equal inputs give equal outputs and nothing is disclosed. That
changes a load-bearing control and wants its own ticket. Until then,
record the blocked join as a `friction` entry and move on. The worked
example does exactly that at step 3.

---

## Driving it

```sh
scripts/runlog.sh init --runbook A1
export STAVE_RUNLOG_DIR='runs/A1-<stamp>'

scripts/runlog.sh step --step 1 \
  --intent "get every open issue so the sweep has a population to age" \
  --criterion "one record per open issue, with severity and created date"

# 1. render the invocation
scripts/runlog.sh canon -- stave list issue --limit 3

# 2. hand that exact text to the stave-safety-coach subagent
# 3. record what it returns
scripts/runlog.sh verdict --coach-file coach.txt -- stave list issue --limit 3

# 4. execute; prints the path to the scrubbed output
scripts/runlog.sh exec --out issues.jsonl -- stave list issue --limit 3

# work done outside stave
scripts/runlog.sh oob --tool jq \
  --purpose "group and count on a composite key of severity and status" \
  --text "jq -s 'group_by([.severity,.status]) | map({k:.[0], n:length})'" \
  --survives-fix no --ticket gs23 \
  --reason "issuesGroupedByValue returns this from the server"

scripts/runlog.sh result --outcome partial \
  --gap "owner attribution: Issue.assignee is unselected (aae-orc-qijl)"

scripts/runlog.sh finish --note "commissioning run"
```

Stream verbs read from `data/`:

```sh
scripts/runlog.sh exec --in "$STAVE_RUNLOG_DIR/data/issues.jsonl" --out open.jsonl \
  -- stave filter --where 'status == "OPEN"'
```

### Exit codes

| Code | Meaning |
|---|---|
| 0 | fine |
| 2 | usage or argv error |
| 3 | run state wrong: no run, halted, missing or out-of-tree input |
| 4 | the gate refused: no matching CLEAR verdict, or the coach reviewed different text |
| 5 | the scrubber refused the output shape; nothing was written |
| 6 | the invocation itself exited nonzero |

4 and 6 are the pair worth separating: "the gate stopped me" and "the
command failed" are different facts about a step, and a driving agent
that conflates them will report the wrong one.

---

## Tests

`scripts/runlog.sh selftest`, wired into `just hygiene` and `just ci`
beside `scrub.sh --selftest`. Synthetic values only, no tenant, no
credentials, no network. The real binary is never invoked: `argv[0]`
resolves to a stub on `PATH`.

Nineteen checks, in four groups.

**The gate.** No verdict on file refuses and the stub is never run
(asserted by a marker file the stub touches on every invocation). A
verdict for a different command does not license this one. A coach block
whose `COMMAND:` line disagrees with the argv is refused. A matching
CLEAR executes. A CLEAR is single use. A file argument from outside
`data/` is refused.

**Scrubbing.** A planted person's name, resource name, and ARN survive
the stub and appear in none of `data/` or `runlog.jsonl`. A positive
control asserts `CRITICAL` did survive, so the check proves scrubbing
rather than deletion. An unclassifiable shape (a rendered table) exits
5, leaves no file, and records the friction.

**Halts.** A HALT latches. A later invocation holding its own CLEAR is
still refused. `resume` clears the latch and records the ruling.

**The `survives_fix` requirement.** Omitting it is an error. Answering
`no` without a ticket is an error. An unknown ticket id is an error. A
well-formed entry records both fields.

**Reconcile.** A clean run reconciles. Running the stub directly, with
the run's session id in the environment and outside the harness, is
detected as a bypass.

What the tests prove is narrower than what the harness claims, in one
place: they prove no CLEAR-less invocation executes, not that a coach
was consulted. That distinction is the subject of "The gate and its
limits" above.

---

## Worked example

`examples/runlog/walkthrough.sh` drives the harness end to end against
`examples/runlog/stub-stave` and prints the runlog. `--write` refreshes
`examples/runlog/example-runlog.jsonl`, which is committed so the mining
stage (`aae-orc-e4jo.7`) can see the shape before any real run exists.

It covers a CLEAR and an execution, a stream stage in `stream` mode, two `out_of_band` entries on opposite sides of `survives_fix`, a
dead end, a friction entry, an unmet result, a HALT with a human ruling,
and a clean reconcile. The coach blocks in it are fixtures, labelled as
such in the script.
