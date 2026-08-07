# finding-002: two paging defects that single-page live validation could not see

Date: 2026-08-06
Probe: pre-run review of the runbook verb-mining exercise (bd aae-orc-e4jo)
Status: both fixed in stave `820a8b2`

## What was found

`stream_kind` in `crates/stave-cli/src/main.rs` is the loop every
`list`, `search`, and `list --since` runs through. It carried two
independent defects, in about forty lines.

**Zero-node page ended the read.** The loop broke out of a connection
whenever a page returned no nodes, even when the same response said
`hasNextPage: true`. Server-side filtering, permission scoping, and
deleted rows all produce that shape. The result was a short read
reported as a complete one: exit 0, nothing on stderr, and a caller
counting records got a number smaller than the truth.

**Page size was derived from the output limit.** The request asked for
`limit - emitted` records. That is correct only when every fetched
record is emitted. `search` and `list --since` filter client-side, so
`emitted` does not move on a non-match and the page stayed pinned at
`--limit` for the length of the connection. `stave search
cloud_resource <rare> --limit 5` read a twenty-thousand-record
connection five records per request: roughly four thousand sequential
round trips against a live production tenant, from a command whose
stated limit is five.

## Why the existing validation missed both

`aae-orc-hzg0` closed on a read-only sweep of all twelve curated kinds,
12/12 returning well-shaped records, and charter F1 was marked
"largely resolved" on that evidence. The sweep ran at `--limit 2`.

Every kind therefore returned exactly one page. Both defects are
properties of the second page onward. The validation commissioned the
first page of each connection and nothing else, and it could not have
found either defect no matter how many kinds it covered. Breadth across
kinds was mistaken for depth through the loop.

This does not retract hzg0. Field selections are live-validated and
that claim stands. What it retracts is the inference that a passing
read means the read path is exercised.

## The test that encoded the defect as design

`list_treats_an_empty_page_as_the_end_of_the_connection` asserted the
first defect as intended behavior, with a comment explaining why it was
correct: "a connection that keeps promising another page while
returning nothing would loop forever; the client breaks instead."

The justification was wrong twice. The cursor advances between pages,
so following it does not loop. And the guard did not prevent the loop
it was written for: when a server does repeat a cursor, the old code
re-emitted the same record until `--limit` cut it off. Verified by
running the new tests against the prior code, where that case produced
fifty copies of a two-record connection.

A wrong test is cheap. A wrong test carrying a plausible rationale is
expensive, because it pre-refutes the next reader. This is the second
instance in the fleet: BetterDials `new_tick_resets_window`
(session-039) encoded a trailing-edge debounce bug the same way, also
with a comment justifying it.

## The safety consequence, which inverts the usual intuition

The `stave-safety-coach` agent gates every invocation against a
production tenant. Its load rule was originally keyed on page count,
which returns CLEAR on the heaviest reads in the tool, because the
second defect made **a small `--limit` worse than a large one**. Load
scaled inversely with the number a reviewer would look at.

The rule is now keyed on the verb (`search`, and `list` carrying
`--since`, both unconditional) rather than on any number. It stays a
HALT after the fix: forty requests is twelve times better than four
thousand and is still a walk of the whole connection. The fix changed
the magnitude, not the kind.

## Bearing on the charter

- **F1.** "Largely resolved" is accurate for field selections and
  overstated for the read path. Live validation to date is
  single-page.
- **F2.** Server-side filter variables are listed as an open question.
  This is evidence for them: as long as `search` and `--since` filter
  client-side, both are full-connection walks by construction, and no
  amount of page-size tuning changes that. The fix reduced the cost of
  the walk; only a server-side filter removes the walk.

## What to carry forward

Commissioning a paginated read means at least three pages, including
one empty page with `hasNextPage: true` and one repeated cursor. Both
are cheap against wiremock and neither is reachable from a live
`--limit 2` probe. `crates/stave-cli/tests/paging.rs` is that harness;
six of its seven tests fail against the prior code, and the seventh is
the regression guard proving the unfiltered path still asks only for
what it needs.
