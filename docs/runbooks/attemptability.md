# Runbook attemptability

bd `aae-orc-e4jo.3` (S1). Settled 2026-08-07 from
`docs/design/field-surface-audit.md`, offline, with no tenant contact.

Consumer is `.14`, the paper pipelines, where a blocked runbook is
written up rather than skipped. What a blocked runbook would need is
itself evidence about the verb surface.

---

## The correction

The ticket proposed this working split, to confirm or revise:

- attemptable now against the tenant alone: A1 through A5, C17, C18,
  part of C14
- attemptable with a synthetic external fixture: B6, B7, B8, B11, B13,
  C16, C20
- blocked without the real external system: B9, B10, B12, C15, C19

**Revised. Zero runbooks are fully attemptable as catalogued.** The split
also assumed one blocking cause when there are two, and conflated them.

C17 and C18 are the sharpest correction. The split called them
attemptable now. They need no external system at all, and they are
blocked completely, by stave's own read surface. They are the purest
available demonstration that the wall is ours.

## Three blocking causes, separated

The original third bucket named only the external cause. This document
then separated a second. Live validation on 2026-08-07 found a third
that fits neither, so there are three:

- **EXTERNAL**: needs an input no security graph holds. Not a defect in
  stave or in Wiz. Fixable here only with a synthetic fixture (`.4`).
- **SURFACE**: needs a field stave does not select, or a root stave does
  not bind. A defect in stave, fixable without the vendor, tracked as
  `aae-orc-rsh6` / `j1xi` / `qijl` / `gs23`.
- **TENANT**: stave selects the field, the document validates, the
  server resolves it, and the tenant returns null. Nobody's defect.
  Nothing in this repo reaches it.

Nothing classifies as blocked by the Wiz API. That result is from the
audit and holds at runbook level too.

### TENANT is not simply a third kind of blocked

A SURFACE block means the step cannot run. A TENANT null means the step
**runs and returns an empty answer**, and whether that satisfies the
runbook depends entirely on what the runbook wanted:

- A step **seeking absence** is satisfied by it. B12 asks which issues
  lack a service ticket. `serviceTickets` null on every record is that
  runbook's answer, not its obstacle.
- A step **seeking attribution** is defeated by it. A1 asks who to
  chase. An owner column of nulls executes perfectly and fails the
  runbook's own success criterion, which is that the operator can walk
  into the monthly and answer who is chasing what.

So TENANT rows carry a per-runbook reading rather than a bucket. Getting
this wrong in either direction is expensive: counted as SURFACE it
inflates apparent verb demand with work that would change nothing;
counted as satisfied it reports a hollow table as a success.

This is the same cut the runbook judges are asked to make between
EXECUTOR SHORTFALL and TOOL CANNOT (`docs/design/runbook-judges.md`
section 5), arriving from the other direction.

### What was measured, and the sample it rests on

Three reads through the harness on 2026-08-07, one sample each, `n = 20`
per kind. That is enough to say "null on every record sampled" and not
enough to say anything about the tenant in general. A differently scoped
service account, or a different slice, may populate any of these.

| Field | Kind | Result | Bears on |
|---|---|---|---|
| `assignee` | issue | null 20/20 | A1 step 4 |
| `serviceTickets` | issue | null 20/20 | B12 step 1 |
| `dueAt` | issue | null 20/20, and 25/25 in an earlier sample | A1 step 3 |
| `projects` | issue | populated 2/20 | project-scoped steps |
| `projects` | vulnerability_finding | null 20/20 | project-scoped steps |
| `vulnerableAsset` | vulnerability_finding | populated 20/20 | A2, A3 |
| `sourceRules` | issue | populated 20/20 | C14 |

`projects` is the instructive row. Null on every finding, populated on
two issues, with the `projects` root returning twenty records. The
nested selection resolves; the finding-side null is an association gap
in the tenant. A single sample of findings would have looked exactly
like a broken selection.

`dueAt` corrects a claim made earlier the same day and propagated to bd
`aae-orc-e4jo.5`: that the server already answers A1's SLA question. It
does not. A1 step 3 computes age from `createdAt` exactly as the runbook
says, which it can, since `createdAt` is selected.

## Classification

"Reachable steps" counts steps executable against today's curated
surface, including steps whose output would be partial.

| ID | Reachable steps | Blocked by | Note |
|---|---|---|---|
| A1 | 4 of 4 | **TENANT** | reclassified 2026-08-07. All four steps now execute: `assignee` is selected, and step 3 computes age from `createdAt`. Step 4 returns null for every issue, so the runbook's own success criterion fails on empty data rather than on a missing capability. `rsh6`/`j1xi`/`qijl`/`gs23` do not recover it |
| A2 | 0 of 4 | SURFACE | needs `vulnerableAsset`, `vulnerabilityExternalId`, and V2 exposure fields |
| A3 | 1 of 4 | SURFACE | narrowing on exposure and sensitive data is V2-only |
| A4 | 2 of 4 | SURFACE | blocked at every step past bucketing by age |
| A5 | 2 of 4 | SURFACE | `SecurityFramework.controls` unselected, so the framework roster has no controls under it |
| B6 | 0 of 4 | SURFACE + EXTERNAL | candidate keys are V2-only, so the graph half cannot be measured either |
| B7 | 0 of 3 | SURFACE + EXTERNAL | |
| B8 | 0 of 4 | SURFACE + EXTERNAL | |
| B9 | 1 of 4 | SURFACE + EXTERNAL | actual enablement is reachable; substantiation needs `lastSuccessfulRunAt` |
| B10 | 0 of 4 | SURFACE + EXTERNAL | audit log carries no actor in the current selection. Measured live 2026-08-07 and it is worse than a selection gap: `performer`, `actionType`, `actionParameters` and `sourceIP` are all selected by the widened document and none of them arrive, on 20 of 20 records, with no GraphQL error. So the surface fix has already been made and did not open the step. Whether this is SURFACE or TENANT is genuinely undetermined, and it is the one row in this table where that is true; the leading hypothesis is silent scope stripping by the server, which would make it neither. See `docs/design/widening-notes.md` queue item 2 |
| B11 | 1 of 4 | EXTERNAL | **the graph half works**: `cloudAccounts` as selected answers step 2 |
| B12 | 1 of 4 | SURFACE + EXTERNAL | step 1 recovered 2026-08-07. It was read as needing the `hasServiceTicket` server-side boolean; `qijl` selected `serviceTickets` instead, and null on every record answers the step directly for a bounded sample. The filter is an efficiency, not a prerequisite |
| B13 | 0 of 4 | SURFACE + EXTERNAL | |
| C14 | 1 of 4 | SURFACE | root-cause fields unselected; the grouping root is unbound |
| C15 | 0 of 4 | SURFACE + EXTERNAL | the IaC fields exist only on V2 |
| C16 | 1 of 4 | SURFACE + EXTERNAL | `firstScannedAt` unselected, so even the scanner-side timestamp is out of reach |
| C17 | 0 of 4 | **SURFACE only** | no external input needed |
| C18 | 1 of 4 | **SURFACE only** | no external input needed |
| C19 | 0 of 4 | SURFACE + EXTERNAL | |
| C20 | 0 of 4 | SURFACE + EXTERNAL | |

## Buckets, revised

**Blocked by our read surface alone, no external system needed (7).**
A2, A3, A4, A5, C14, C17, C18. These become attemptable when `rsh6`,
`j1xi`, `qijl`, and `gs23` land, with no fixture work and no external
dependency.

**A1 leaves this bucket**, and it is the correction that costs the most,
because A1 was the strongest single case for the claim that our own
backlog recovers a large slice of the catalogue. Its blocking step now
executes and returns nulls, so `rsh6`/`j1xi`/`qijl`/`gs23` do not
recover it.

The heading was already wrong before that. It read `(7)` over a list of
eight IDs, and the prose beneath it said "eight of twenty". Removing A1
happens to make the count seven and the list agree for the first time,
which is luck rather than arithmetic and is recorded so nobody reads the
matching numbers as confirmation of the revision.

**Blocked by the tenant's own data, and by nothing we can fix (1).**
A1. Named as its own bucket rather than folded into a footnote, because
it is the only runbook here whose remedy lies outside this repo
entirely. Two routes exist and neither is engineering: the tenant starts
populating issue assignees, or A1 is rewritten to attribute through
something that is populated, such as `projects` (2 of 20) or
`sourceRules` (20 of 20). The second is a catalogue change and belongs
to whoever owns the runbook, not to stave.

**Needs a synthetic external fixture, and a surface fix (11).** B6, B7,
B8, B9, B10, B12, B13, C15, C16, C19, C20. A fixture alone does not make
these runnable, which is a change from the original split: every one has
a surface block on its graph half as well. Building fixtures before the
surface work would produce runs that fail for the wrong reason.

**Needs a synthetic external fixture only (1).** B11. Its graph half is
already served. If the gate opens and exactly one fixture is built, this
is the one that yields a real attempt.

## What this changes downstream

For `.14`: every runbook is writable on paper, and for most of them the
pipeline's blocked stages are blocked by a field selection rather than by
an absent capability. That distinction is what the survives-the-fix
tagging captures.

For `.5`: the commissioning run will execute partial runbooks. Expected,
not a finding. Revised 2026-08-07: **do not commission on A1.** Its
blocking step now runs and returns nulls, so a commissioning run against
it cannot distinguish a working instrument from a broken one. Pick a
runbook whose steps produce non-empty output.

For `.8`, the verb proposal, and this is the reclassification's whole
point: **a TENANT wall argues for no verb.** A SURFACE wall says build
the binding or pass the filter. A TENANT null says the data is not
there, and no verb, primitive, or composite changes that. Counting the
two together inflates apparent demand with work that would ship and
alter nothing. A1 was the single strongest demand signal in the class-A
set and it is now evidence of nothing about the verb surface.

For `.10`, the judges: a TENANT row is neither EXECUTOR SHORTFALL nor
TOOL CANNOT as those are defined in `docs/design/runbook-judges.md`
section 5. The tool did what was asked and the executor did nothing
wrong. Judges reaching an A1-shaped verdict need a third answer, or
section 5 needs a sentence telling them which of the two to file it
under and why. Worth settling before the first judging pass rather than
during it.

For `.4` and `.6`, if the gate opens: fixture work has a prerequisite it
did not appear to have. Ten of the eleven fixture-class runbooks stay
blocked after the fixture is built.
