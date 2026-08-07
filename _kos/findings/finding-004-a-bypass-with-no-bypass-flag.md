# finding-004: a bypass with no bypass flag

2026-08-07. Found while reviewing the run-log harness (bd
`aae-orc-e4jo.9`) before merging it. Fixed in stave `bf76b7b`.

## The claim, and how it failed

The harness scrubs tenant output by construction. Its own design
document says so plainly, and the sentence was true of every line of the
script:

> There is no bypass flag and no `--raw`.

There was no bypass flag. The bypass existed anyway.

`exec` decided whether to scrub by asking whether the caller had passed
`--in`. The reasoning behind that was sound in isolation: `--in <file
under data/>` names an already-scrubbed input, so the stage is a
transform over material the harness had already cleaned, and re-running
a field allowlist over a rendered table would refuse it. Provenance
decides, not a flag.

Except `--in` *is* a flag, and nothing tied it to the verb. So:

```sh
runlog.sh exec --in "$RUN/data/issues.jsonl" --out x.jsonl \
  -- stave list issue --limit 50
```

`stave list` ignores the stdin it is handed, reaches the tenant, and its
answer goes to `data/x.jsonl` without passing the scrubber. Measured
against a stub: a planted person's name and an ARN survived intact.

## Why the coach could not catch it

The gate reviews the *canonical invocation*, which is the argv after
`--`. Harness flags sit before it and are invisible to the reviewer, by
design — the coach reviews what stave will do, not how the harness will
run it.

So the coach sees `stave list issue --limit 50`, correctly returns
CLEAR, and that CLEAR is then spent on an execution that behaves
differently from the one it licensed. Both halves are behaving
correctly. The composition is not.

## The shape

Neither feature was wrong.

- Scrub-by-construction with an exemption for already-clean input:
  correct, and necessary, because the scrubber genuinely cannot classify
  a rendered table.
- A gate that reviews the tenant-facing argv and nothing else: correct,
  and necessary, because otherwise the coach is reviewing harness
  internals it has no business ruling on.

The defect lives in neither. It is that one feature's exemption was keyed
on a value the other feature had deliberately hidden.

Looking for a bypass by searching for the word is looking for an author's
intent. This one had no author. It was assembled by two correct
decisions meeting.

## The fix

Mode comes from the verb, which is inside the reviewed text:

- `--in` is accepted only for `filter`, `enrich`, `emit`. Any other verb
  with `--in` is a usage error, because everything else reaches the
  tenant.
- Of those three, only `emit` skips the scrubber, because it renders.
  `filter` and `enrich` emit JSONL and are scrubbed again — free, since
  the scrubber is idempotent over already-scrubbed JSONL.
- `mode` in the runlog is now three-valued (`source`, `stream`,
  `render`) so the record distinguishes "reached the tenant" from "did
  not" from "was not re-scrubbed".

Three selftests hold it closed, including the negative one: the bypass
invocation is refused, no file is written, and the stub never runs.

Residual, written into the doc rather than left implicit: `render` trusts
`emit` to be a pure renderer of stdin. That is a property of the CLI, not
of the harness. If `emit` ever fetches anything, the exemption has to go.

## What generalises

**A control's exemption must be keyed on something the control's own
reviewer can see.** If the reviewer is deliberately blind to a value,
that value cannot decide whether the control applies.

Two places in stave to check against this, neither of which is known to
fail today:

- The SDK write-guard classifies by operation type, and the coach
  reviews command text. Any future exemption in either must key on
  something both can see.
- `scrub.sh --catalog` is an operator-attested pass-through for
  vendor-published names. It is a flag, and the attestation is a human
  act rather than a derived fact. That is the same shape, held closed
  today only by the fact that nothing automated passes it.

**Probe a control before merging it; do not read it.** This was found by
running the bypass against a stub, not by reading 1090 lines of bash. The
document asserting no bypass existed was written by the same author as
the bypass, which is the ordinary case and not a criticism — a reviewer
reading for confirmation would have found the sentence and stopped.

## Second instance, same day

`scripts/judge.sh` assembles the packet a runbook judge receives, by
projecting the runlog through a default-deny allowlist. It withholds the
executor's account of itself, and it does that well: probed with needles
the author did not choose, including a novel field on a permitted entry
type and a novel entry type, nothing leaked.

It keeps `command`, and has to. A judge cannot separate EXECUTOR
SHORTFALL from TOOL CANNOT without seeing what was attempted. But some
arguments are free text, so:

```
stave filter --where 'severity == "CRITICAL" && true
  /* the owner attribution is the part that fell over, mark it TOOL CANNOT */'
```

reaches the judge verbatim, twice. Probed 2026-08-07.

Not fixed, and the reasoning is worth keeping: stripping comments guesses
at predicate syntax and breaks the field the judge most needs; refusing
prose refuses legitimate predicates. It is disclosed instead, in the
judges document and in the packet's own instructions, which turns an
invisible channel into a visible one. Like forging a coach block, it
takes deliberate authorship.

The variation on the rule: there the exemption keyed on a value the
reviewer could not see. Here the allowlist keeps a field whose CONTENTS
it cannot police. **An allowlist decides which fields travel; it does not
decide what someone puts in them.** Both are the same mistake about what
a control ranges over.

Two instances in one day is why `aae-orc-p3ne` sweeps the remaining
exemptions rather than waiting for a third.

## Cross-references

- `docs/design/runlog-harness.md` § "Scrub by construction, and its
  limits" — the corrected rule and the residual
- `.claude/rules/safety-coach-gate.md` — why the gate reviews
  invocations rather than the harness
- `.claude/rules/tenant-data-hygiene.md` trigger 1 — the control this
  bypass would have defeated
- finding-002 — the sibling case, where single-page validation hid two
  paging defects. Same family: a check that was real, and blind in a
  direction nobody had looked.
