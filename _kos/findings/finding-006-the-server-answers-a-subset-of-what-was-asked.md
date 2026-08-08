# finding-006: the server answers a subset of what was asked

2026-08-07, run `QIJL-VALIDATION-20260807T081109Z`, steps 7 and 9. Two
kinds measured, both at `--limit 20`, read-only, under the coach gate.
Not explained. bd `aae-orc-cw9y` carries the study.

## What was measured

On two of the twelve curated kinds, fields that the document selects do
not come back. `result: ok`, `graphql_error: null`, HTTP fine, no
partial-data marker.

| Kind | Selected | Arrived | Missing |
|---|---|---|---|
| `audit_log` | 8 | 4 | `actionType`, `actionParameters`, `sourceIP`, `performer` |
| `cloud_account` | 11 | 6 | `firstScannedAt`, `criticalSystemHealthIssueCount`, `highSystemHealthIssueCount`, `linkedProjects`, `sourceDeployments` |

Twenty records each, no record carrying a missing field.

## The sweep: all twelve kinds, and the gap is exactly two

Six kinds already had data, so that half cost nothing. The remaining six
were read once each in run `FIELD-SWEEP-20260808T010628Z` (bd
`aae-orc-k5o7`), bounded at `--limit 20`, under the coach gate,
reconciled 6/6 with no bypass. Top-level node fields only.

| Kind | Selected | Arrived | Missing |
|---|---|---|---|
| `issue` | 19 | 19 | — |
| `vulnerability_finding` | 16 | 16 | — |
| `project` | 9 | 9 | — |
| `security_framework` | 6 | 6 | — |
| `report` | 2 | 2 | — |
| `control` | 8 | 8 | — |
| `user` | 7 | 7 | — |
| `service_account` | 9 | 9 | — |
| `cloud_resource` | 6 | 6 | — |
| `cloud_config_rule` | 5 | 5 | — |
| `audit_log` | 8 | 4 | four, above |
| `cloud_account` | 11 | 6 | five, above |

**Ten of twelve are complete.** `report` returned 1 record and
`service_account` 12; both are tenant counts below the limit, not
truncation.

So this is not a general property of the tenant, the transport, or
stave's pipeline — none of those would spare nineteen fields on `issue`
and then drop five on `cloud_account`. It is **confined to two kinds**,
and field-level within them.

That is a much better answer than the one this finding opened with. The
caveat below is no longer "no kind is known-clean until swept" — ten are
now known-clean, and an absent field on any of them means what it says.

One shape worth recording without a theory attached, because it is the
kind of pattern that invites one. On `cloud_account` the six that arrive
are the five pre-widening fields minus the deprecated `status`, plus
exactly the two (`lastScannedAt`, `resourceCount`) that were live-checked
on 2026-08-06 when `status` was corrected. The five that never arrive are
the five never checked against the live server. This is circular as
evidence — a field is "checked" because it arrived — so it is recorded as
a coincidence to test, not a mechanism.

## What it is not

Each of these was ruled out before the finding was written, because the
alarming reading was available first and I took it first.

**Not redaction.** `actionType` and `criticalSystemHealthIssueCount` are
both on `scrub.sh`'s field allowlist. A denied field arrives as
`<redacted:name>` with its key intact; these have no key at all.

**Not the scrubber dropping keys.** Verified against a synthetic audit
record: every key survives, `actionType: null` stays null, and
`actionParameters` becomes the redaction string. Both keys present.

**Not nulls being elided somewhere in the pipeline.** In the same run,
against the same binary, `issue` carries `assignee`, `serviceTickets`
and `dueAt` as explicit JSON nulls, 20/20. Nulls travel.

**Not a per-kind projection in stave.** `crates/stave-sdk` has none.

**Not a stale binary, and this is the one I got wrong first.** Reading
`binary_version: dev+g08a5350-dirty` against `repo_commit: 06abc1b`, I
concluded mid-run that the whole run had executed pre-widening documents
and that `docs/runbooks/attemptability.md`'s measured table was void.
It had not and it was not. The disproof is arithmetic rather than
argument:

- pre-widening `cloudAccounts` selected `id name externalId cloudProvider status`
- what arrived was `id name externalId cloudProvider lastScannedAt resourceCount`

The arrived set **contains** two fields only the widened document
selects and **lacks** the one field only the old document selected. No
stale binary produces that. `strings` on the running binary finds
`criticalSystemHealthIssueCount`, `actionParameters`, `vulnerableAsset`
and `sourceRules`, and the binary was built at 03:00 from a tree already
carrying the 02:10 widening commit. The `-dirty` suffix means built with
uncommitted changes present, which is why the harness's skew check
compares mtimes there instead of shas — correctly, and it passed.

## The hypothesis, and the observation that undermines it

Silent scope stripping: a server that drops fields the service account
cannot read, rather than erroring on them, produces exactly this shape.
It is attractive because charter F1 already records that this same
service account does not expose readable granted scopes, so this would
be one phenomenon seen from two sides.

Against it, on the same type:

- `resourceCount` arrives; `criticalSystemHealthIssueCount` does not
- `lastScannedAt` arrives; `firstScannedAt` does not

Two counts on one type on opposite sides of the line, and two timestamps
on one type on opposite sides of the line. Whatever partitions these,
"scopes" is not an obvious fit.

The sweep pushes back on it further. A scope model would have to grant
this account all nineteen fields on `issue`, all sixteen on
`vulnerability_finding`, and then split `cloud_account` down the middle.
Not impossible, since `linkedProjects` and `sourceDeployments` do reach
other resources, but `firstScannedAt` against `lastScannedAt` is hard to
tell a scope story about.

The honest state is that the hypothesis survives without being
supported, and the sweep made it less likely rather than more.

## Why this matters more than the queue item that found it

It changes what a class of already-published readings can mean.

**Unsafe, but now on exactly two kinds:** "field absent, therefore the
tenant has no data" holds on the ten clean kinds and does not hold on
`audit_log` or `cloud_account`, where absence has two possible causes
and the run cannot tell them apart. This is the version after the full
sweep; before it the caveat had to be read as applying everywhere, which
is what made the sweep worth doing ahead of the mechanism.

**Unaffected:** "key present with an explicit null." That is a real
answer from the server about a field it chose to return.

The measured table in `docs/runbooks/attemptability.md` rests entirely
on the second form — `assignee`, `serviceTickets`, `dueAt`, `projects`,
`vulnerableAsset` and `sourceRules` all arrive as keys — so it stands.
That was checked before this finding was written, not after, because the
whole point of the finding is that absence has stopped being
self-explanatory.

The one row that does change is B10, whose surface fix has already
shipped and did not open the step, and which is now the only row where
SURFACE versus TENANT is undetermined.

## The discriminator, and a cheaper route to it than first thought

The original plan was to run the same two reads under a service account
with known-broader scopes and diff the key sets. That works, and it
depends on someone provisioning a second account.

The sweep turned up a route that may not: **`ServiceAccount.scopes`
arrives and is populated, 12 of 12.** That is exactly the field bd
`aae-orc-8af5` exists to probe, opened because charter F1 records that
the token itself would not expose granted scopes. The scrubber withholds
the values, so this establishes only that the field resolves and is
non-empty — which is the first thing `8af5` asks.

If our own account's granted scopes are readable that way, the
correlation this finding needs (do the missing fields line up with
scopes we lack?) becomes testable against the account we already have.
That would remove the provisioning dependency from the discriminator
entirely. Stated as a route to try, not a result: nothing here has read
a scope value.

Either way, no write of any kind.

## A method note that is not incidental

Both the alarm and its disproof came from the same habit: check the
version binding before interpreting a surprising absence. The harness
records `binary_version` and `repo_commit` precisely so that check is
one command. What it did not record was **which** skew test had decided,
so a deliberately-skipped sha comparison looked like a missed one. Fixed
in `910dac7` — `run_start` now carries `skew_basis` — and the fix exists
because a passing control that cannot say why it passed cost most of an
hour and nearly produced a retraction of a correct document.

## Cross-references

- `docs/design/widening-notes.md` queue items 2 and 4
- `docs/runbooks/attemptability.md` — the B10 row, and the measured table
- bd `aae-orc-cw9y` (scope-qualification study, carries the discriminator),
  `aae-orc-8af5` (`ServiceAccount.scopes` as a readable-scope route)
- charter F1 — scope qualification did not manifest with this account
- `elem-control-scope-matches-reviewer-scope` — a fourth instance of a
  control ranging over something other than what its reader assumed,
  this time the skew record rather than the check itself
