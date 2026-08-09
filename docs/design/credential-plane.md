# The credential plane: purpose-built least-privilege service accounts

Design document. 2026-08-08.

bd: `aae-orc-oqg2` (umbrella), `aae-orc-oqg2.1` (the gate),
`aae-orc-oqg2.2` (the minting account's own requirements).

**Status: proposed, not ratified.** Phase 0 is committed and tracked in
bd. Phases 1 through 4 are the plan of record and are deliberately NOT
in bd. They are filed as tickets when each phase is committed to, per
`.claude/rules/task-workflow.md`: bd is layer 3, for work we intend to
close on a timeframe. A plan is not a commitment, and a tracker full of
speculative phases reads as one.

---

## Why, and the baseline this replaces

Current practice is handing out overpowered service accounts. This is
not an inference. Measured on this tenant 2026-08-08: the development
credential holds **79 scopes including `read:all`** against a declared
requirement of twelve, an excess of 67.

That number is the yardstick for everything below.

So the risk comparison is not mint-capability versus no-mint-capability.
It is mint-capability versus standing over-provisioning, and the
standing practice is worse on every axis: broader grants, no expiry, no
purpose record, no inventory, no revocation path. A credential factory
that issues narrow, expiring, purpose-named accounts is a net reduction
in tenant privilege even counting its own risk surface.

Two consequences follow, and neither is cosmetic.

**Success is not "stave can mint."** It is *the population of live
credentials gets narrower over time*. That makes inventory and
revocation load-bearing rather than hygiene, and it makes `excess: 67`
the baseline of a metric worth tracking.

**Self-service is the mechanism of change, not scope creep.** Issuing
one narrow account changes nothing about practice. If the
least-privilege path is slower than asking for an overpowered account,
people will keep asking for the overpowered account, correctly, because
they have work to do. The design constraint that follows: **least
privilege must be the path of least resistance.** That is a usability
requirement carrying a security payoff.

This work is scoped deliberately outside the verb creation and testing
process (`aae-orc-e4jo`). It directly supports that work, and it is
what makes stave usable and safe beyond it.

---

## The vendor surface

Verified in the vendored schema, offline, no tenant contact.

```graphql
createServiceAccount(input: CreateServiceAccountInput!): CreateServiceAccountPayload!

input CreateServiceAccountInput {
  name: String!
  scopes: [String!]
  assignedProjectIds: [String!]
  expiresAt: DateTime
  description: String
  type: ServiceAccountType
}

type CreateServiceAccountPayload {
  serviceAccount: ServiceAccount
}
```

`INTEGRATION` is a `ServiceAccountType` value, so integrations and
service accounts are the same object here. The separate
`createIntegration` mutation is for connectors (ServiceNow, Slack,
Snowflake) and is not this.

`assignedProjectIds` and `expiresAt` give two narrowing axes beyond
scopes, which is what makes the least-privilege story real rather than
nominal.

`deleteServiceAccount` exists in the same namespace. That is both the
revocation path and part of why the planes are split.

---

## Architecture

**Two planes, two binaries, one SDK, one repo.**

- **Read plane** (`stave`). Unchanged. D1 stands: every mutation and
  subscription refuses unconditionally, no flag, env, or config lifts
  it. This stays a byte-stable, testable property.
- **Credential plane** (new binary, same workspace). Service-account
  lifecycle only. Own keyring entry under a distinct service name, own
  credential, own audit stream. **Reads no posture data**: the curated
  read operations are not linked into it.

D1 is not lifted, narrowed, or given an override surface. The
credential plane is a different binary with a different credential.
That is the whole point: an agent holding the read tool cannot escalate
by discovering a flag, because the capability is not in that binary.

Same repo and workspace because the SDK already carries what this
needs (OAuth mint, token cache, audit JSONL, keyring custody,
resolution chains), and because the sibling pattern here is
repo-per-vendor, not repo-per-plane.

### Known collision

`DENIED_SELECTIONS` blocks `ServiceAccount.clientSecret` at build time.
`CreateServiceAccountPayload` returns `ServiceAccount`, and the secret
is `String!` on that type. The credential plane needs its own selection
policy permitting that field in exactly one code path, which writes to
custody and never to stdout.

### Bootstrap

The first minting account is created by hand in the console. The
factory cannot mint itself. Worth naming so it surprises nobody.

---

## Phases

### Phase 0, offline. Committed, in bd.

`aae-orc-oqg2.1` is **the gate**. Emit the exact `createServiceAccount`
mutation and variables for an account granted exactly the twelve scopes
the operation registry declares, and explicitly not `read:all`.

The twelve, emitted today by `stave auth plan`:

```
admin:audit                 read:reports
read:cloud_accounts         read:resources
read:cloud_configuration    read:security_frameworks
read:controls               read:service_accounts
read:issues                 read:users
read:projects               read:vulnerabilities
```

Eleven are `read:*`. `admin:audit` is the only one that is not, which
is why it is currently the only declared name validated. See the
finding-007 correction, `aae-orc-h9uo`.

Also emitted: name per convention, description carrying requesting
human and justifying ticket, `expiresAt` always set, `type`, and
optionally `assignedProjectIds`.

**A second variant in the same pass: the same twelve minus
`read:projects`.** That is the `aae-orc-i8cj` discriminator. If
withholding a grant makes fields vanish silently rather than erroring,
the silent-scope-stripping hypothesis is confirmed and a P1 closes; if
the call errors, the hypothesis dies cleanly. The current
over-privileged account can produce neither outcome.

Why offline first: the output is executable by hand in the console
immediately, so the measurement credential waits on nothing else, and
the account is right the first time rather than approximated.

Alongside it, `aae-orc-oqg2.2` carries the two questions that gate
provisioning the minter at all. Both are likely faster to answer from
the admin console than to probe, and neither is derivable from the
schema, which publishes no scope requirements for any root:

1. **Can a Wiz service account grant scopes it does not itself hold?**
   This gates the whole custody design. If it cannot, the minting
   account is maximally privileged by construction. If it can, the
   minting account can be narrow but is a privilege-escalation
   primitive and must be treated as one.
2. **What scope permits `createServiceAccount`?** By the tenant's
   naming convention, likely `create:service_accounts` or
   `admin:service_accounts`.

Also in Phase 0, not yet filed: the profile model and input validation.
A named profile resolving to a scope set, an expiry default, optional
project narrowing, and a required purpose string. Validation refuses an
unnamed account, an unbounded expiry, and an empty scope set.

### Phase 1, first real mint

The credential-plane binary, then one live call.

Gated by mandatory expiry, mandatory confirmation, safety-coach review
of the exact invocation, and a full audit line. Target is the
twelve-scope measurement account, so proving the path and obtaining the
credential we need are the same call.

Success criterion is not "a call succeeded." It is: the resulting
account has exactly twelve scopes, no `read:all`, a set expiry, and a
purpose record, verified by reading it back through the read plane's
`list_service_accounts`.

### Phase 2, custody and control together

Secret captured once from the payload and written straight to the
platform keyring, and optionally to Keeper for distribution. Never to
stdout, a file in the tree, a commit, or a transcript. Tests assert the
secret appears in no stdout, no audit line, and no error path.

**Inventory and revocation ship here, not in Phase 4**, and the
sequencing is deliberate: a credential factory without a shredder
produces a population that grows faster than anyone tracks it, and
self-service is exactly the step that accelerates it.

Inventory is nearly free because `list_service_accounts` already reads:
who minted what, for what purpose, with which scopes, expiring when.
That view is what makes narrowing measurable, and narrowing is the
success metric.

### Phase 3, self-service

**Profiles, not free-form scopes.** A user picks a reviewed template
(`vuln-mgmt-analyst`, `read-only-auditor`) and never types a scope
string. Free-form composition by end users reintroduces every
escalation risk the two-plane split removes.

Carries mandatory expiry, project narrowing via `assignedProjectIds`,
and a purpose record per issued credential.

This crosses SOUL §3's auth boundary (your credentials, your agents,
versus minting for other people's agents on machines we do not
control), so the identity and audit story has to be carried from Phase
1 rather than added here.

### Phase 4, distribution at team scale

Keeper shared folders, rotation, revocation workflow, and the practice
metric reported back: how the live credential population narrowed
against the `excess: 67` baseline.

---

## Operating modes: interactive, batch, unattended

The intended use is not one operator minting one account. It is batches:
driven by tickets, by a harness session enrolling a group of users, or by
CI. That changes which controls still work, so it belongs in the design
rather than in Phase 4.

### Enrolment is not minting, and there are three lifetimes

Enrolment is local credential management, so any binary may do it.
Minting a token is session establishment: it presents the secret and
leaves the process holding a usable session for that identity, so a read
binary must not do it for a provisioning profile. That is why
`plan_login` computes the plane check before `verify` rather than after.

Three separate clocks, and only one of them is short:

| Thing | Lifetime | Stored | Ended by |
|---|---|---|---|
| OAuth access token | server's `expires_in`, default 3600s, treated stale 300s early (`EXPIRY_MARGIN_SECS`, `token.rs:34`) | XDG state cache, mode 0600 | expiry, `auth logout`, `auth login` |
| Client secret | none, valid until rotated | platform keyring, `client-secret:<profile>` | a human rotating it |
| Service account | `expiresAt`, mandatory in this design | the tenant | expiry, or `deleteServiceAccount` |

So yes, the minted token expires, and `auth login` clears the cache so a
token minted from a superseded secret cannot outlive it
(`main.rs:1099`). The row without an expiry is the middle one, and it is
the row that grants everything. That is the argument for short
`expiresAt` on issued accounts: it is the only clock we control.

### Four controls assume a human at a terminal

Each one has to be replaced, not merely waived, before a batch runs.

1. **The safety coach does not exist in CI.** It is an LLM subagent
   invoked from an interactive session. A batch harness has no such
   reviewer, so the invariants worth checking have to be expressible as
   code that runs in-process. The ones that cannot be expressed that way
   are exactly the ones a human must approve in advance. This is the
   load-bearing gap in the whole batch story and it is not yet in bd;
   this paragraph is the layer-1 record.
2. **Phase 1's mandatory per-call confirmation** has nobody to confirm.
   Replace it with per-batch pre-approval: an input manifest naming every
   account with its scopes, expiry, project narrowing and purpose,
   reviewed once by a human, with the run refusing any account not in the
   manifest. Confirmation moves from per-call to ahead-of-time, which is
   also the only form that scales past a handful.
3. **"A provision-plane profile may not come from the stored default"
   loses its force.** A workflow file that passes `--profile provisioner`
   on every step is a stored default with extra syntax. The refusal was
   designed against an operator who forgot which profile was active, and
   that failure mode does not exist unattended. The plane split still
   holds, because it is a binary boundary rather than a habit.
4. **The hidden secret prompt inverts.** CI needs the minter's secret in
   the environment, which is the shape deliberately removed for humans,
   and it is a standing high-privilege credential sitting where a lot of
   automation can read it.

Problem 4 has an answer the design already contains: give the minter
itself a short `expiresAt` and re-mint it per campaign from the bootstrap
account. A leaked CI secret then expires on its own, and the minting
capability is not standing. That makes the factory recursive in a useful
direction rather than an alarming one.

Ticket-driven batches fit this well: the ticket is the manifest entry.
It already carries the requesting human and the justification, which is
what `description` is specified to hold, so inventory becomes traceable
back to a request without anyone writing it down twice.

---

## Open decisions

**The escalation question** (`aae-orc-oqg2.2`, question 1) changes the
custody design more than anything else here and should be answered
first.

**Create and delete in one account, or two?** One credential-lifecycle
account is simpler. Splitting the shredder from the factory limits the
blast radius of a compromised minter. Current lean: one account, with
delete gated harder at the CLI. Not settled.

**Naming convention.** Proposed `stave-<purpose>-<owner>-<yyyymm>`,
with `description` carrying the requesting human and the justifying
ticket. The payoff is that `list_service_accounts`, which stave already
reads, becomes a self-describing inventory of everything the factory
ever produced.

---

## What this obliges elsewhere

The charter and the graph need to record the two-plane split, or a
future session reads D1 plus the "not a remediation console" non-goal
and correctly concludes the credential plane is a defect.

Shape when the plane ships: a bedrock node for the split, with D1
restated exactly as it stands and scoped explicitly to the read plane.
Charter gets a short pointer only, per
`.claude/rules/charter-light-touch.md`. The non-goal stays true and
gains an explicit carve-out: minting credentials is not remediating
findings.

---

## Cross-references

- `aae-orc-oqg2` and children; `aae-orc-h9uo` (finding-007 correction)
- Unblocked by the gate: `aae-orc-cw9y`, `aae-orc-vhje`, and via the
  second variant `aae-orc-i8cj`
- `docs/design/read-only-posture-and-permissions-report.md` (D1, D3,
  the maps-vs-walls split)
- `_kos/findings/finding-007-a-second-route-to-our-own-scopes.md`
- `charter.md` B4 (read-only posture), B5 (registry credential
  custody), and the "not a remediation console" non-goal
