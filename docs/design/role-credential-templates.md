# Role credential templates

Six starting templates for purpose-built, least-privilege Wiz service
accounts, derived from the operator runbooks in
`docs/runbooks/catalogue.md` rather than invented.

2026-08-09. Companion to `docs/design/credential-plane.md` (the minting
architecture) and `docs/design/measurement-account-request.md` (the
mechanics of one mint).

**Status: proposed.** One template has been minted and verified
(`measurement`, which is not a business role). The six here are designs.
The scope sets are computed from `required_scopes` in the operation
registry, which is provisional per `SCOPE_METADATA_PROVISIONAL`: the
names are validated, the per-operation assignment is not.

---

## Why this document exists

Current practice is handing out `read:all`. Measured on this tenant: the
development credential holds 79 scopes including `read:all`, against a
declared requirement of twelve.

`read:all` is not a modest default. On this tenant it covers 75 read
scopes: every cloud account, every resource, every unremediated finding,
every portal user, every service account. Handed to an AI-augmented team
member, it is a complete targeting map of the enterprise's cloud estate,
and it is handed over because enumerating the right subset is harder than
not.

The templates below make enumerating the subset the easy path. That is
the design constraint from `credential-plane.md` restated concretely:
**least privilege must be the path of least resistance**, or people will
keep asking for the overpowered account, correctly, because they have
work to do.

## How the vendor's own model frames this

Wiz ships built-in user roles along two axes, a global one and a project
one: **Global Admin**, **Global Reader**, **Project Admin**, **Project
Member**, plus custom roles composed of granular permissions.

> Sourcing caveat: `docs.wiz.io` returned HTTP 429 when fetched on
> 2026-08-09, so this list comes from secondary sources rather than the
> vendor's documentation read directly. Treat the role names as
> approximately right and verify before quoting them to anyone.

Two things follow for integrations.

`Global Reader` is the user-side equivalent of `read:all`. The habit this
document is trying to break is not unique to service accounts; it is the
same habit one layer over.

The project axis is the part worth shadowing. `assignedProjectIds` on
`CreateServiceAccountInput` is the service-account analogue of **Project
Member**, and it is a second narrowing dimension that costs nothing and
is currently unused in every credential we hold.

---

## The three axes

A credential template is not just a scope list. Three things narrow it,
and only the first is usually discussed.

1. **Scopes.** What kinds of thing it can read at all.
2. **Projects** (`assignedProjectIds`). Which slice of the estate.
   Shadows Wiz's Project Member role. **DEFERRED, bd `aae-orc-t5cd`.**
3. **Expiry** (`expiresAt`). For how long. Mandatory in this design,
   never unbounded.

> **Axis 2 is deferred and nothing here should rely on it.** We do not
> yet know whether we will use project narrowing at all, how this tenant
> uses projects, or which scopes can even be narrowed by them. The
> "Projects:" line on each template below records what the role's shape
> *suggests*, not a design decision, and no template should be minted
> with `assignedProjectIds` set until `aae-orc-t5cd` is answered.
> **Ship templates scope-narrowed and expiry-bounded only.** The scope
> axis alone already takes the spoke-team template from 79 to 7, so
> nothing important waits on this.

Two roles with identical scope sets can be different credentials. An
incident responder and a standing vulnerability analyst need the same
reads; the responder gets a short expiry and no project narrowing,
because during an incident the blast radius is the question. That is a
different credential, not a different scope list.

---

## The six templates

Grant sets computed from `required_scopes` in
`crates/stave-api/src/lib.rs`. "Withheld" is every other declared scope,
and it is the more interesting column: it is the boundary the credential
enforces.

### 1. Spoke-team remediation owner

The IT admin who receives the tickets. Class C of the runbook catalogue
is defined as exactly this ("run by the teams receiving the tickets") and
its seven runbooks need four graph objects between them: issues,
vulnerability findings, cloud resources, cloud accounts.

- **Runbooks:** C14 root-cause collapse, C15 fix-at-source, C16 account
  enrollment lifecycle, C17 regression tracking, C18 resolved vs
  evaporated, C19 exception round-trip, C20 asset claiming.
- **Operations:** `list_issues`, `list_vulnerability_findings`,
  `list_cloud_resources_v2`, `list_cloud_accounts`.
- **Grant (7):** `read:cloud_accounts`, `read:issues`, `read:projects`,
  `read:resources`, `read:service_accounts`, `read:users`,
  `read:vulnerabilities`.
- **Withheld (6):** `admin:audit`, `read:cloud_configuration`,
  `read:controls`, `read:permission_scopes`, `read:reports`,
  `read:security_frameworks`.
- **Projects:** yes. This is the template that most needs project
  narrowing, because a spoke team owns a slice.
- **Expiry:** long, this is a standing role. 90 days, renewed.

**Aspirational variant, 5 scopes.** See "The identity tension" below.
Drop `read:users` and `read:service_accounts`.

### 2. Vulnerability analyst

Central triage across the estate, not a single team's slice.

- **Runbooks:** A1 remediation SLA sweep, A3 toxic combination triage,
  plus C14 and C17 read centrally.
- **Operations:** `list_issues`, `list_vulnerability_findings`,
  `list_cloud_resources_v2`.
- **Grant (6):** `read:issues`, `read:projects`, `read:resources`,
  `read:service_accounts`, `read:users`, `read:vulnerabilities`.
- **Withheld (7):** the six above plus `read:cloud_accounts`.
- **Projects:** no. The role is cross-project by definition.
- **Expiry:** 90 days.
- **Aspirational variant, 4 scopes.**

**Incident-response variant.** Same scopes, expiry measured in days
rather than months, no project narrowing. A2 emergency blast radius. The
template differs on axes 2 and 3 only, which is the clearest illustration
that scopes are not the whole credential.

### 3. Compliance evidence

GRC. Reads the control and framework surface and nothing about
individual resources or people.

- **Runbooks:** A5 framework evidence pull, B9 control assertion
  reconciliation.
- **Operations:** `list_controls`, `list_security_frameworks`,
  `list_reports`, `list_cloud_configuration_rules`.
- **Grant (4):** `read:cloud_configuration`, `read:controls`,
  `read:reports`, `read:security_frameworks`.
- **Withheld (9):** `admin:audit`, `read:cloud_accounts`, `read:issues`,
  `read:permission_scopes`, `read:projects`, `read:resources`,
  `read:service_accounts`, `read:users`, `read:vulnerabilities`.
- **Projects:** no, and it holds no `read:projects` to narrow with.
- **Expiry:** aligned to the audit cycle.

This is the cleanest separation in the set. A compliance credential that
cannot enumerate a single cloud resource or person is a genuinely
different thing from `Global Reader`, and it does the job.

### 4. Identity and credential auditor

Runbook A4, standing credential review. The role that reviews the very
credentials this document creates.

- **Operations:** `list_service_accounts`, `list_users`,
  `list_audit_log_entries`.
- **Grant (3):** `admin:audit`, `read:service_accounts`, `read:users`.
- **Withheld (10):** everything posture-facing.
- **Projects:** no.
- **Expiry:** short. It holds the only non-read scope in the vocabulary.

Note what it cannot see: not one issue, finding, resource or cloud
account. An identity auditor has no business reading the estate's
security posture, and this credential enforces that rather than trusting
it. It is also the mirror of template 1, which is the argument for the
whole exercise: two real roles in one tenant with almost disjoint needs,
both of whom would today be handed `read:all`.

### 5. Inventory reconciler

Asset and CMDB work. Sees what exists, not what is wrong with it.

- **Runbooks:** B7 CMDB three-bucket, B8 ownerless-resource cross-check,
  B11 scan coverage gap, B13 decommission verification, C16.
- **Operations:** `list_cloud_resources_v2`, `list_cloud_accounts`.
- **Grant (3):** `read:cloud_accounts`, `read:projects`,
  `read:resources`.
- **Withheld (10):** including every finding and identity scope.
- **Projects:** yes.
- **Expiry:** long, standing integration.

The interesting property: it can enumerate the estate but learns nothing
about its weaknesses. That is the right shape for a CMDB sync, and it is
the shape most likely to be over-granted in practice because "it just
needs inventory" sounds harmless and `read:all` is one click.

### 6. Reporting consumer

The narrowest useful credential in the set, and the one to hand a
dashboard or an exec summary job.

- **Operations:** `list_reports`.
- **Grant (1):** `read:reports`.
- **Withheld (12):** everything else.
- **Projects:** no.
- **Expiry:** long.

One scope against seventy-nine. Worth showing people first, because it
makes the point without argument.

---

## The identity tension, and why it is the next thing to test

`list_issues` declares four scopes: `read:issues`, `read:projects`,
`read:users`, `read:service_accounts`. The last two are there because the
document selects `assignee`.

So a spoke-team credential built faithfully from our own registry lets an
IT admin enumerate **every portal user and every service account in the
tenant**. That is the problem this document exists to solve, arriving
from our declarations rather than from `read:all`, and it is worse in one
respect: it looks least-privilege while carrying the tenant's whole
identity surface.

It inflates three of the six templates:

| Template | As declared | Aspirational |
|---|---|---|
| Spoke-team remediation | 7 | **5** |
| Vulnerability analyst | 6 | **4** |
| Incident response | 6 | **4** |

**The test.** Mint template 1 with the aspirational five and let the
server adjudicate. By `finding-009` the server names required scopes in
its denials, so either outcome is a result: if issues return, our
registry over-declares and gets corrected; if it denies, the server names
exactly what it wanted and the declaration is validated instead of
assumed.

**The consequence if the denial is real**, which is the part worth taking
to the IS team: least privilege at the credential layer is **capped by
what our documents select**. Giving a spoke team a credential that cannot
enumerate users would require an issues document without `assignee`, not
merely a narrower grant. That is a product decision about the read
surface, not a permissions decision, and no amount of careful scoping
reaches it.

---

## What is not yet settled

**Project narrowing is deferred, not merely unknown.** Tracked as bd
`aae-orc-t5cd`, which carries five separate unknowns: which scopes are
project-narrowable (`PermissionScope.isProjectScope`, one read away),
whether this tenant's projects correspond to anything a role boundary
should follow, whether `assignedProjectIds` on a service account behaves
like Project Member does on a user, what happens to a pinned credential
when projects are reorganised, and whether the axis is worth its
operational cost at all. That last one has a legitimate answer of "no".

**The scope assignments are provisional.** `SCOPE_METADATA_PROVISIONAL`
is `true`. Twelve names are validated (finding-008) and one assignment is
server-validated (finding-009, `read:permission_scopes`). The other
thirteen are our declarations. `finding-009`'s method makes them readable
at one call each with a deliberately under-privileged credential, which
is the cheapest way to put these templates on evidence.

**No template here has been minted.** They are computed from the
registry, not measured against the tenant.

---

## Minting one

Mechanics are in `docs/design/measurement-account-request.md`: the
`createServiceAccount` mutation, hand-executed in the console, then
enrolled with `stave profile add` plus `stave auth login`, then verified
with `scripts/scope-membership.sh`.

One change for a role template: substitute the role's grant list for the
twelve. Leave `assignedProjectIds` omitted, as the measurement account
did, until `aae-orc-t5cd` is answered.

Whether this set should become a first-class artifact rather than a
document, definable and tunable and used by a provisioning process, is
captured as an idea rather than a plan:
`_kos/ideas/role-template-catalogues-for-provisioning.md`.

Naming follows `credential-plane.md`: `stave-<role>-<owner>-<yyyymm>`,
with `description` carrying the requesting human and the justifying
ticket, so `list_service_accounts` becomes a self-describing inventory of
every credential the factory ever issued.

## Cross-references

- `docs/design/credential-plane.md`, `docs/design/measurement-account-request.md`
- `docs/runbooks/catalogue.md` (the source of every operation set above)
- `_kos/findings/finding-008-...`, `_kos/findings/finding-009-...`
- `charter.md` F1 and `SCOPE_METADATA_PROVISIONAL`
- bd `aae-orc-oqg2`, `aae-orc-kuqt`, `aae-orc-cw9y`
