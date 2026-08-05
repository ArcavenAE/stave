# stave recipes

Three Wiz workflows written as shell pipelines over the v0.1
primitives. They exist to answer one question ahead of v0.2: does the
primitive set compose into the composite verbs Wiz operators actually
want, or is a primitive missing? Charter F4 is where the answer lands,
and the audit trail is the evidence it will be argued from.

The rule these follow is `.claude/rules/cli-philosophy.md`, "one thing
well": a primitive transforms a stream once, and a composite stays a
recipe until usage evidence earns it a place in the binary.

## The three recipes

| Recipe | Question it answers | Pipeline |
|---|---|---|
| `issue-triage.sh` | What needs attention right now, and on which resource? | `list issue` → `enrich entity-hoist` → `filter severity` → `emit md` |
| `vuln-exposure.sh` | What is still open across the estate, by severity? | `list vulnerability_finding` → `enrich severity-roll-up` → `filter status` → `emit md` |
| `resource-inventory.sh` | What do we own, and which account owns it? | `list cloud_account` + `list cloud_resource` → `enrich account-context` → `emit md` |

Run one: `bash examples/recipes/<name>.sh`.
Run all: `for r in examples/recipes/*.sh; do bash "$r"; done`.

Each needs a configured `stave` (client credentials with read scopes).
They read the live tenant and write nothing: every verb in them is a
read, so the write-guard never comes up.

## What the three shapes are for

Each recipe leans on a different enrichment shape, which is the point.
The recipes and the recipe library were designed against each other.

- **hoist** (`entity-hoist`): Wiz nests the affected resource inside
  `issue.entitySnapshot`. A table renderer and a CEL predicate both
  want it flat, so the hoist runs before the filter in
  `issue-triage.sh`. Without it, the triage table names issues without
  naming what they are about.
- **roll-up** (`severity-roll-up`): findings report `vendorSeverity`,
  issues report `severity`. The roll-up writes both into
  `severity_rollup` so one predicate and one column serve a stream
  drawn from either kind. This is what makes a cross-kind posture view
  writable at all.
- **join** (`account-context`): ownership lives in a different kind
  than the resource, keyed `subscriptionExternalId` to `externalId`.
  The join is client-side, which is why `resource-inventory.sh` pulls
  accounts into a temp file first.

## The md table cannot show an enriched field

Running these against the fixtures surfaced a concrete gap. Two of the
three recipes enrich a stream and then render it, and the enrichment
does not survive the render: `emit --format md` builds a fixed
four-column table (`_kind`, `id`, `severity`, `timestamp`) from the
kind table's metadata, so `entity_name` and `account` are present in the
stream and absent from the output. Only `vuln-exposure.sh` reads
correctly as written, because the column it cares about is the kind's
own severity field.

Measured 2026-08-05 with `target/debug/stave` against
`../fixtures/cloud_resource.jsonl`: four resource rows, every
`severity` and `timestamp` cell empty, no account column. The join is
verifiably there in `--format jsonl`, and `examples/asserts/02` proves
it, which is how the gap showed up as a rendering problem rather than an
enrichment one.

The recipes work around it in a comment: `emit --format json`, which
pretty-prints one array of whole records and is the readable way to
confirm an enrichment landed, or a jq projection when you want a
specific column. The fix is a v0.2 question with three candidate shapes:
`emit --columns a,b,c` for explicit projection; md widening to render
every top-level scalar it finds; or a per-recipe declaration of the
columns the recipe adds. Explicit projection is the one that does not
make the table shape depend on the stream's contents, which matters for
a tool whose stdout is a contract.

## What these recipes do not reach

Two shapes recur in the questions operators ask and do not compose from
`{list, get, search, filter, enrich, emit}`:

- **Set difference over two streams.** "Which issues opened since last
  week's report, which closed, which are still open" is a keyed diff.
  No arrangement of the six primitives produces it; the shell can fake
  it with `comm` on sorted id lists, which loses every other field.
- **Reading state as of an earlier point.** Any week-over-week question
  needs yesterday's stream, not just today's. The audit trail was
  designed with this in mind: `trace_id` clusters one invocation's
  calls and `shape_hash` identifies same-shaped responses over time, so
  a later replay verb has something to key on.

Both are v0.2 candidates, and both are inherited observations rather
than stave discoveries: the sibling repos hit them first. Neither
blocks v0.1, whose target is exactly the triage and inventory flows
above.

## Aggregation bends

`vuln-exposure.sh` renders every open finding. The question behind it
is usually "how many of each severity", which is an aggregate the
primitive set does not have. Three ways out, in ascending cost:
`emit --format summary` as an output mode; a `count` primitive as
`filter`'s cousin; or leaving it to `jq 'group_by'` in the shell. The
audit trail should decide, once it shows how the flow is actually
composed. Recorded here rather than settled.

## Cross-references

- `charter.md` B4 (write-guard), F1 (live validation), F4 (composite
  verbs from audit-trail evidence)
- `.claude/rules/cli-philosophy.md` (one thing well; stdout is the
  contract)
- `.claude/rules/tenant-data-hygiene.md` (why recipe output is never
  pasted into an issue verbatim)
- `../fixtures/` and `../asserts/` (the same flows, on synthetic data,
  with no credentials)
