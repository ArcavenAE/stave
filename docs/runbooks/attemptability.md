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

## Two blocking causes, separated

The original third bucket named only the external cause. Both exist and
they behave differently, so they get separate labels:

- **EXTERNAL**: needs an input no security graph holds. Not a defect in
  stave or in Wiz. Fixable here only with a synthetic fixture (`.4`).
- **SURFACE**: needs a field stave does not select, or a root stave does
  not bind. A defect in stave, fixable without the vendor, tracked as
  `aae-orc-rsh6` / `j1xi` / `qijl` / `gs23`.

Nothing classifies as blocked by the Wiz API. That result is from the
audit and holds at runbook level too.

## Classification

"Reachable steps" counts steps executable against today's curated
surface, including steps whose output would be partial.

| ID | Reachable steps | Blocked by | Note |
|---|---|---|---|
| A1 | 3 of 4 | SURFACE | only owner attribution fails, and it is the step the runbook is for |
| A2 | 0 of 4 | SURFACE | needs `vulnerableAsset`, `vulnerabilityExternalId`, and V2 exposure fields |
| A3 | 1 of 4 | SURFACE | narrowing on exposure and sensitive data is V2-only |
| A4 | 2 of 4 | SURFACE | blocked at every step past bucketing by age |
| A5 | 2 of 4 | SURFACE | `SecurityFramework.controls` unselected, so the framework roster has no controls under it |
| B6 | 0 of 4 | SURFACE + EXTERNAL | candidate keys are V2-only, so the graph half cannot be measured either |
| B7 | 0 of 3 | SURFACE + EXTERNAL | |
| B8 | 0 of 4 | SURFACE + EXTERNAL | |
| B9 | 1 of 4 | SURFACE + EXTERNAL | actual enablement is reachable; substantiation needs `lastSuccessfulRunAt` |
| B10 | 0 of 4 | SURFACE + EXTERNAL | audit log carries no actor in the current selection |
| B11 | 1 of 4 | EXTERNAL | **the graph half works**: `cloudAccounts` as selected answers step 2 |
| B12 | 0 of 4 | SURFACE + EXTERNAL | step 1 is a server-side boolean stave does not pass |
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
A1, A2, A3, A4, A5, C14, C17, C18. These become attemptable when
`rsh6`, `j1xi`, `qijl`, and `gs23` land, with no fixture work and no
external dependency. That is eight of twenty runbooks recoverable
entirely from stave's own backlog.

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
not a finding.

For `.4` and `.6`, if the gate opens: fixture work has a prerequisite it
did not appear to have. Ten of the eleven fixture-class runbooks stay
blocked after the fixture is built.
