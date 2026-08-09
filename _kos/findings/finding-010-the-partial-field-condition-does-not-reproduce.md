# finding-010: the partial-field condition does not reproduce

2026-08-09, bd `aae-orc-i8cj` (P1) and `aae-orc-cw9y`. Four live reads,
all read-only, each coach-cleared and executed through the run harness.
Run reconciled clean: six invocations, six audit lines, zero bypasses.

## What was recorded

finding-006 measured two of twelve curated kinds returning a proper
subset of the fields their documents select, with `result: ok`, no
GraphQL error and no partial-data marker:

| Kind | Selected | Arrived | Missing |
|---|---|---|---|
| `audit_log` | 8 | 4 | actionType, actionParameters, sourceIP, performer |
| `cloud_account` | 11 | 6 | firstScannedAt, criticalSystemHealthIssueCount, highSystemHealthIssueCount, linkedProjects, sourceDeployments |

That observation opened `question-partial-field-resolution`, whose
leading hypothesis was silent scope stripping, and it is the premise of
`aae-orc-i8cj`.

## What is true now

Both kinds return everything their documents select, under both
credentials, at `--limit 20`:

| Kind | Selected | Arrived | Credential |
|---|---|---|---|
| `audit_log` | 8 | **8** | `measurement`, twelve scopes, no `read:all` |
| `audit_log` | 8 | **8** | `reader`, 79 scopes including `read:all` |
| `cloud_account` | 11 | **11** | `measurement` |

All nine previously missing fields are present. They are not empty keys:
`performer` and `actionParameters` arrive populated and are redacted by
the scrubber, `actionType` carries values, and `sourceIP` is populated on
5 of 20 records and an **explicit null** on the other 15. That last
detail matters on its own: the server distinguishes null from absent, so
the earlier absence was not null-elision.

`cloud_resource_v2` was measured the same way in the same run, under both
credentials. Field population was identical on all 27 fields, including
exact agreement on the partially populated ones (`cloudAccount` 8 and 8,
`cloudPlatform` 13 and 13, `providerUniqueId` 9 and 9). So the 67 extra
scopes and `read:all` buy nothing there either.

## The two cheap explanations, both checked and both unavailable

**The documents changed after the finding.** They did not. The widening
commit `39337d7` (2026-08-07 02:10) is an ancestor of finding-006's
commit `6ffb696` (09:00 the same day). The documents were already wide
when the measurement was taken.

**A stale binary asked for less than the tree selects.** finding-006
recorded ruling this out, by observing that its arrived set contained two
fields only the widened document selects and lacked the one field only
the old document selected. That is a specific check and it holds.

So the difference is unexplained.

## What this does and does not settle

It does **not** show finding-006 was wrong. Two measurements taken two
days apart disagree, and a transient server-side condition is consistent
with both. The finding recorded its rule-outs carefully and they survive.

It does show that `aae-orc-i8cj`'s premise **does not currently
reproduce**, which changes what that ticket can be worked on. A P1 whose
symptom is not present cannot be diagnosed by looking harder; it can only
be watched for, or reproduced deliberately.

It also weakens silent scope stripping specifically, from the direction
nobody planned. The narrow credential sees exactly what the broad one
sees on all three kinds measured. If fields were being stripped by grant,
twelve scopes against seventy-nine plus `read:all` is where it would
show, and it does not.

**Axis, per corollary 1 of [[elem-validation-scope-matches-claim-scope]]:**
three kinds, one page each at `--limit 20`, two credentials, one moment
on 2026-08-09. It does not range over the other ten kinds, over deeper
pages, or over time. A condition that appeared once and vanished is
exactly the kind that a single clean run cannot rule out, and this run
does not claim to.

One honest gap in my own measurement: I cannot compare method with
finding-006 beyond what it wrote down, because its sample was not
retained. Which is the practice note below.

## Practice note: keep the sample

finding-006's sweep predates the run harness, so its records are gone and
this had to be written as a contradiction rather than a diff. Had both
runs kept their output, the difference would be a `jq` invocation rather
than an open question. `scripts/runlog.sh` retains every invocation's
scrubbed output under `data/` by construction, which is the fix already
in the tree. The lesson is to route field measurements through it rather
than running them by hand, and it costs nothing to say so now.

## What follows

- `aae-orc-i8cj` is re-scoped rather than closed: premise does not
  reproduce, symptom absent, watch rather than diagnose.
- `question-partial-field-resolution` stays frontier and gains the
  non-reproduction plus a sub-question about transience.
- Variant B of the measurement-account request drops in value. It was
  designed to test withholding `read:projects` against a symptom that is
  not currently present, and `finding-009` supplies a cheaper route to
  the assignment question it was a proxy for.

## Cross-references

- `_kos/findings/finding-006-the-server-answers-a-subset-of-what-was-asked.md`
- `_kos/findings/finding-009-the-server-names-the-scope-it-wanted.md`
- `_kos/nodes/frontier/question-partial-field-resolution.yaml`
- `_kos/nodes/bedrock/elem-validation-scope-matches-claim-scope.yaml`
- `docs/design/measurement-account-request.md` (variant B)
- Run: `runs/cw9y-scope-sweep-20260809T023946Z/`
