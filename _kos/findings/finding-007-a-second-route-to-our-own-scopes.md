# finding-007: a second route to our own scopes

2026-08-08, bd `aae-orc-8af5`. Closes the one item finding-001 left
genuinely open. Two live calls, both read-only, both coach-cleared.

## What was open

finding-001 found that this tenant's service-account tokens carry
`encodedScopes`, a base64 bitmask against an internal ordering stave
does not have. `token_scopes` therefore reports `Opaque`, and
`auth scopes`, `auth can-i` and `auth plan --check` refuse to answer
rather than emit a verdict they cannot support — option (a), ratified.
Charter F1 has carried that as its remaining open item ever since.

The refusal was right. It was also a dead end: no amount of decoding
gets a grant list out of a bitmask whose ordering is not published.

## The second route

`ServiceAccount.scopes` is a `[String!]!` on a type stave already
queries, and `clientId` is already in the same selection. So the
question the token would not answer can be asked of the directory
instead: find the record whose `clientId` is ours, read its scopes.

The 2026-08-08 field sweep established the field arrives and is
populated (12 of 12 records). This finding is the other half: the
caller's own account is in that connection, and its scopes are
readable.

    stave auth scopes --from-directory
    → exit 0, {"source": "directory", "scope_count": 79}

## What it settled, precisely

Run against the live tenant, reading counts only:

    stave auth plan --check --from-directory
    → {"required": 12, "granted": 79, "missing": 0, "excess": 67}

**`missing: 0` is the decisive number.** Every one of the twelve scope
names stave's operation registry declares exists in the tenant's actual
granted set. The names were chosen from documentation and have been
marked provisional since; twelve of them are now real strings this
server recognises.

`excess: 67` says the development credential is broadly over-privileged,
and exit 1 is correct: that is real drift, reported as drift. It is not
a defect in the check.

## What it did NOT settle, and why the flag stays true

`SCOPE_METADATA_PROVISIONAL` stays `true`. The temptation to flip it is
exactly the kind of overclaim the flag exists to prevent.

Validated: twelve scope **names** exist in this tenant's vocabulary.

Not validated: the **assignment**. Knowing `read:projects` is a real
scope says nothing about whether `list_issues` genuinely requires it —
the registry's per-operation mapping was reasoned from field selections,
not measured. A credential holding 79 scopes cannot distinguish a
correct mapping from a generous one, because everything succeeds either
way. Also unvalidated: the `read:all` implication rule (D3).

The measurement that would settle the mapping is a least-privilege
credential, granted exactly the twelve, run against every operation. The
excess of 67 is precisely what makes this account unable to answer it.

## Design decision: opt-in, not fallback

bd `aae-orc-8af5` step 3 said to wire the directory as a fallback inside
`token_scopes` so `Readable` could be reached by a second route. That
was not done, deliberately.

`token_scopes` is a pure function on a token string, and `auth scopes`
has always been an offline read of the token at hand. A silent fallback
would turn both into network calls — breaking the determinism rule in
`.claude/rules/cli-philosophy.md` ("same argv + same stdin + same config
→ same stdout") and surprising anyone who reasonably assumed a scope
dump costs nothing.

So the route is `--from-directory` on `auth scopes`, `auth can-i` and
`auth plan --check`, defaulting off. The SDK half is
`stave_sdk::own_scopes`, which returns three outcomes rather than an
`Option`: `Found`, `Empty` (the server answered about us and said none)
and `SelfNotListed` (we could not see ourselves). Collapsing the last
two would send someone to the wrong fix.

**Naming the flag in the opaque-scopes message is not the wall/ladder
problem.** `cli-philosophy.md` forbids a guard refusal from naming what
would lift it. This is a *map* error — the caller is lost and there is a
legitimate next step — and maps are required to name it.

## Hygiene, since the directory is every account in the tenant

`own_scopes` returns on the first `clientId` match and drops every other
record unexamined; non-matching records are counted and nothing more.
Two tests assert it rather than trusting it: one in the SDK on the
matcher, one through the binary asserting a neighbour's scopes and
client ID never reach stdout.

Neither live call brought the tenant's grant configuration into a
transcript. Both were projected to counts before being read — `{source,
scope_count}` and `{required, granted, missing, excess}` — because
`scripts/scrub.sh` fails closed on `auth` output and could not be used.
That refusal is correct for the scrubber and is a real gap in the run
harness, filed as bd `aae-orc-2gk8`: the harness cannot execute any
non-stream verb, so this validation ran under the coach gate with no
harness enforcement behind it.

## Cross-references

- finding-001 (`f1-live-validation`) — the open item this closes
- finding-006 — the sweep that established `scopes` arrives populated
- charter F1, B3 (the resolution chains), B4 (the permission layers)
- bd `aae-orc-8af5` (this), `aae-orc-cw9y` (the scope-qualification
  study), `aae-orc-i8cj` (whose discriminator this may unblock),
  `aae-orc-2gk8` (the harness gap)
- `crates/stave-sdk/src/directory.rs`,
  `crates/stave-cli/tests/directory_scopes.rs`
