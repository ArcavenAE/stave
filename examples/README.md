# examples: fixtures, asserts, recipes

Regression surface for the v0.1 primitive layer, ported from bloomctl
and re-instantiated on Wiz security-graph nouns.

```
examples/
├── fixtures/
│   ├── issue.jsonl                 4 records (4 severities, 3 with an
│   │                               entitySnapshot, 1 resolved with none)
│   ├── vulnerability_finding.jsonl 5 records (4 severities, 2 sharing HIGH
│   │                               so the rank tiebreak is exercised)
│   ├── cloud_resource.jsonl        4 records (2 accounts, 1 orphan subscription)
│   └── cloud_account.jsonl         2 records (join targets: AWS + Azure)
├── asserts/
│   ├── _lib.sh                     shared helpers (binary probe, normalize)
│   ├── 01-round-trip.sh            parse + filter(_kind) + emit is lossless
│   ├── 02-cross-kind-enrich.sh     cloud_resource → cloud_account join,
│   │                               orphan subscription yields null
│   └── 03-rank-stability.sh        severity ordering is deterministic,
│                                   CRITICAL first, tiebreak on firstDetectedAt
└── recipes/
    ├── issue-triage.sh             list issue | entity-hoist | filter severity | md
    ├── vuln-exposure.sh            list vulnerability_finding | severity-roll-up | md
    └── resource-inventory.sh       list cloud_resource | account-context | md
```

## Running

```sh
make -C examples assert     # jq + shell, no network, no credentials
```

The asserts are jq simulations of the primitive flows, so they run with
no binary and no tenant. When a locally built binary is present
(`cargo build`, giving `target/debug/stave`) each simulation is also
cross-checked against what the real primitive emits on the same
fixtures. A binary that predates a recipe is reported and skipped
rather than failed, so a mid-rewrite tree still gets a green signal
from the jq half.

The same fixtures back the Rust integration tests in
`crates/stave-cli/tests/`, so the two surfaces cross-check each other.

The recipes need a configured `stave` (client credentials with read
scopes) and demonstrate primitive composition. They are the seed shapes
for the v0.2 composite-verb question; see `recipes/README.md`.

## What each assert proves

| Assert | Claim |
|---|---|
| `01-round-trip` | Every fixture parses, and `filter --where '_kind == "X"'` over a single-kind stream returns it unchanged. Catches renamed fields, wrong types, a `_kind` that disagrees with its filename, and any drift between the jq contract and `stave filter`. |
| `02-cross-kind-enrich` | The `account-context` join resolves `cloud_resource.subscriptionExternalId` against `cloud_account.externalId`, attaches the account summary (id, name, externalId, cloudProvider, status), yields `account: null` for a subscription no account claims, and leaves records of other kinds untouched. |
| `03-rank-stability` | The severity ordering is deterministic across runs, CRITICAL sorts first and LOW last, and equal severities break the tie on `firstDetectedAt` descending. The severity-int mapping mirrors `enrich::severity_rank`, so a change on either side shows up here. |

## Fixture notes

| kind | what it exercises |
|---|---|
| `issue` | the four severities; `entitySnapshot` for the `entity-hoist` recipe, including a resolved issue whose snapshot is `null`; `createdAt`/`updatedAt`/`resolvedAt`/`dueAt` timestamp promotion, with nulls on the fields Wiz leaves empty |
| `vulnerability_finding` | `vendorSeverity` as the severity carrier (not `severity`); severity rank ordering with a real tiebreak; `firstDetectedAt` and `lastDetectedAt` promotion; a mix of OPEN / IN_PROGRESS / RESOLVED |
| `cloud_resource` | the join key (`subscriptionExternalId`), two owned subscriptions, one orphan reference, and two cloud platforms |
| `cloud_account` | the join target (`externalId`), an AWS numeric account id beside an Azure subscription GUID, and two connection states |

**Fixtures are synthetic by policy.** Never regenerate them by copying
live payloads: real account ids, resource names, subscription ids, and
emails must not enter git (SECURITY.md "Tenant Data Hygiene"). Every
value here is invented: `example-corp-*` names, a reserved-looking
`123456789012` account id, GUIDs from a zeroed range. If a live payload
disagrees with a fixture's *shape*, synthesize new placeholder values
matching the corrected shape and fix the kind table together with it.

One deliberate omission: `vulnerability_finding` records carry no
`subscriptionExternalId`, because
`crates/stave-api/ops/list_vulnerability_findings.graphql` does not
select one. Fixtures mirror the curated selection set exactly; adding a
field the query never returns would make the regression contract lie. If
that operation grows the field, add it here in the same change.
