# Verb candidate registration and scoring predicate

bd `aae-orc-e4jo.13` (S0d). Registered 2026-08-07, **before** any paper
pipeline (`.14`), any run, and any scoring.

This document exists so the scoring predicate cannot be tuned to promote
the verbs the exercise already favours. Everything below is fixed at
registration. If the predicate turns out to be the wrong one, that is
outcome 2 of the gate (`.15`) and the correct response is to stop and
rewrite it in the open, not to adjust it mid-scoring.

---

## 1. The priors

Named by the same session that designed the exercise that will evaluate
them. Stated here unchanged from bd `aae-orc-e4jo`.

| # | Verb | Argument shape as registered |
|---|---|---|
| P1 | `join` | two streams, a key, and a mode: `inner`, `left-anti`, `right-anti` |
| P2 | `roll-up` | a stream, a grouping key, an aggregate |
| P3 | `coverage` | two streams and a key; reports the fraction that correspond |
| P4 | `diff` | two streams and a key; reports per-field disagreement |
| P5 | `reconcile` | two streams and a key; reports correspondence, absence in each direction, and disagreement together |

`join --left-anti` is recorded in the umbrella as the leading candidate.
That expectation is registered here so it is visible as a prior belief
rather than discovered as a conclusion.

## 2. The decoys

Drawn from generic stream-tool vocabulary with no runbook provenance.
Stated unchanged from the ticket.

| # | Verb | Argument shape as registered |
|---|---|---|
| D1 | `sort --by` | a stream, one or more field keys, direction |
| D2 | `dedupe` | a stream, a key or key set |
| D3 | `pivot` | a stream, a row key, a column key, a cell aggregate |
| D4 | `topn` | a stream, a rank field, a count |
| D5 | `explain` | a record or stream; reports how a derived value was arrived at |
| D6 | `watch` | a stream expression, an interval |
| D7 | `diff --since` | one stream at two times, a key |

**The decoys are the measurement, not filler.** If they score comparably
to the priors under the identical predicate, the predicate is measuring
"is this a plausible stream verb" rather than "did role-cast elicitation
find something", and the elicitation added nothing a generic vocabulary
would not have supplied.

**Some decoys may legitimately win.** `sort --by` and `topn` are real
gaps in a JSONL tool and may well earn their place. A decoy scoring well
is a result about the tool, not a defect in the decoy set. It is recorded
as a result.

### Name collision in the registered sets, ruled at registration

`D7 diff --since` shares a name with `P4 diff`. Since overlap and
scoring are defined over verb name **plus** argument shape, the two are
distinct entries and are scored separately. But the collision is a real
measurement hazard: any judgement that leans on name similarity will
credit D7's score to P4, or the reverse.

**Ruling.** D7 and P4 are scored as distinct entries. Any conclusion that
rests on D7 is reported separately and explicitly, and is never pooled
into a "decoys scored X" summary without that note. If D7 scores well and
P4 does not, that is evidence about temporal comparison as a capability
and is not evidence for the `diff` prior.

## 3. The scoring predicate

Applied identically to all twelve entries. Every input is computed from
`.14`'s paper pipelines, so the scoring is mechanical rather than a
matter of opinion.

A verb **qualifies** when all three conjuncts hold.

### Conjunct 1: breadth. Appears in **N = 3** or more runbooks.

Three of twenty is fifteen percent. A verb appearing in one or two
runbooks is bespoke to those runbooks and does not earn a place in the
verb set; a threshold much above three would reject verbs that are
genuinely load-bearing for a single class of the catalogue.

"Appears in" is decided from the paper pipeline for that runbook, not
from opinion: the verb appears if `.14`'s pipeline for that runbook uses
it, or is written with a marked gap where it would be used if it existed.
`.14` is instructed to mark those gaps regardless of whether the verb is
a prior or a decoy.

### Conjunct 2: compression. Collapses **M = 3** or more steps.

Counted per runbook as the number of stave invocations plus named glue
stages in that runbook's paper pipeline that the verb would replace. A
verb scores M for a runbook, and the qualifying M is its **median across
the runbooks where it appears**, not its maximum. The median is chosen
because a maximum rewards one spectacular case and a mean is dragged by
it.

Three is set rather than two because two is the arithmetic minimum for
the word "collapse" to mean anything, and a bar at the arithmetic minimum
is not a bar.

### Conjunct 3: absorption. Absorbs client-side glue of a named kind.

The kinds are enumerated here so the conjunct is falsifiable. A verb
satisfies conjunct 3 when the glue it absorbs is of at least one kind
below, named explicitly in the scoring:

| Kind | Definition |
|---|---|
| G1 correspondence | matching records across two streams on a key |
| G2 aggregation | grouping records and computing a per-group value |
| G3 set difference | presence or absence of a key between two streams |
| G4 temporal comparison | the same logical record compared at two times |
| G5 derivation | computing a field from other fields across a stream |
| G6 ordering and selection | ranking a stream and taking a subset |

A verb that absorbs no glue of any named kind fails conjunct 3 even if it
is convenient. If a verb absorbs glue that fits none of G1 to G6, the
kind is added to this table **with a note that it was added during
scoring**, and the addition is reported at the gate. Adding a kind
silently is the failure mode this enumeration exists to prevent.

### Scoring is blind to set membership

The scorer receives all twelve entries as one shuffled list with the
prior and decoy labels stripped, scores them, and the labels are
reattached afterwards. The mapping is held in this document and not given
to the scorer.

This costs nothing and removes the most obvious route by which a prior
gets the benefit of the doubt.

## 4. Recorded alongside the score, not part of it

For each of the twelve, record whether the Wiz API already exposes a
server-side root field that does the same job. Established by the field
surface audit (`docs/design/field-surface-audit.md`, bd
`aae-orc-e4jo.16`): `issuesGroupedByValue` and its siblings, the `*Trend`
and `*HistoryEvents` roots, `securityFrameworksDiff`, and the
`filterBy` and `orderBy` arguments stave does not currently pass.

This is **not** a fourth conjunct and does not disqualify a verb. A stave
verb that fans out across kinds and emits one stream is not the same
object as a single server root field. But a verb whose entire value is
client-side re-implementation of an available server field is a different
proposition from one with no server analogue, and the gate needs to see
which is which.

It applies identically to both sets. At least P2 `roll-up` and P4 `diff`
have server analogues, and so do D3 `pivot` and D7 `diff --since`.

## 5. What a result looks like

Reported at the gate as one table of twelve rows: verb, argument shape,
runbook count, median compression, glue kinds absorbed, qualifies yes or
no, server analogue yes or no. Set membership is a column added after
scoring.

Three readings are pre-named:

- **Priors qualify, decoys largely do not.** The elicitation surfaced
  something a generic stream vocabulary would not have. The exercise did
  what it claimed.
- **Decoys qualify comparably.** The predicate is measuring plausibility,
  not elicitation. This is outcome 2 of the gate and the response is to
  stop and rewrite the predicate in the open.
- **Neither set qualifies.** The predicate is too strict, or the paper
  pipelines are not detailed enough to score against. Also a stop, and
  the diagnosis is which of the two before any rewrite.

None of the three is a failure of the exercise. The exercise fails only
if the predicate is adjusted after the numbers are visible.
