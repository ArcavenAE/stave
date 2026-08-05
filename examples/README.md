# examples — fixtures, asserts, recipes

Track B-style regression surface, ported from sidestep and
re-instantiated on iru nouns.

```
examples/
├── fixtures/
│   ├── device.jsonl         4 records (2 blueprints + 1 orphan, mixed platforms,
│   │                        one stale last_check_in, asset tags on 2)
│   ├── blueprint.jsonl      2 records (join targets for blueprint-context)
│   ├── vulnerability.jsonl  3 records covering the severity range
│   └── audit-event.jsonl    2 records (occurred_at timestamps)
├── asserts/
│   ├── 01-round-trip.sh         parse + filter(_kind) + emit is byte-identical
│   ├── 02-cross-kind-enrich.sh  device.blueprint_id ↔ blueprint.id joins work,
│   │                            orphans yield null
│   └── 03-rank-stability.sh     severity ranking is deterministic, critical first
└── recipes/
    ├── inventory.sh         list devices | enrich blueprint-context | emit md
    ├── stale-devices.sh     list devices | filter last_check_in staleness | emit md
    └── vuln-triage.sh       list vulnerabilities | filter severity | emit md
```

## Running

```sh
make -C examples assert     # jq + shell, no network, no credentials
```

The asserts simulate the primitive flows in jq so they run without the
binary; the same fixtures back the Rust integration tests
(`crates/stave-cli/tests/`), so the two surfaces cross-check each
other.

The recipes require a configured `stave` (token + subdomain with
read permissions) and demonstrate primitive composition — they are the
seed shapes for the v0.2 composite-verb question (charter F3).

## Fixture notes

| kind | what it exercises |
|---|---|
| `device` | join key (`blueprint_id`), orphan case, `last_check_in` promotion, `has(record.asset_tag)` absence checks, platform variety |
| `blueprint` | join target, DRF-wrapper list shape |
| `vulnerability` | severity enum + rank ordering, `*_date` timestamp promotion |
| `audit_event` | `*_at` timestamp promotion, action search field |

**Fixtures are synthetic by policy.** Never regenerate them by copying
live payloads — real serials, UDIDs, device names, and emails must not
enter git (SECURITY.md "Tenant Data Hygiene"). If a live payload
disagrees with a fixture's *shape*, synthesize new placeholder values
that match the corrected shape, and fix the kind table together with it.
