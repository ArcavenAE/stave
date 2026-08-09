# Role template catalogues as a provisioning primitive

Pre-hypothesis. Filed 2026-08-09 from an operator observation while
writing `docs/design/role-credential-templates.md`. No commitment; this
is a shape worth looking at, not a decision.

## The observation

Six role templates were derived in one sitting from an existing runbook
catalogue, and deriving them was mechanical: take the operations a role's
runbooks need, union their `required_scopes`, and the withheld set falls
out as the complement. The hard part was not computing the sets. It was
knowing which operations a role needs, which came from
`docs/runbooks/catalogue.md`, a document written for an entirely
different purpose.

The operator's reaction was the useful part: *maybe there are catalogues
of definable or tunable named role templates that a provisioning process
could define and use, to help the Wiz admin.*

## What that would mean

Today `credential-plane.md` Phase 3 says "profiles, not free-form
scopes": a user picks a reviewed template and never types a scope string.
That is the right constraint and it is stated as a UI rule. This idea
makes the template a **first-class artifact** rather than a convention:

- **Definable.** A template is data, not prose in a design document.
  Name, purpose, the operations it needs, the derived grant set, project
  posture, expiry policy, and the runbooks or business function it traces
  to.
- **Tunable.** A tenant starts from a shipped catalogue and adjusts. Two
  organisations' "vulnerability analyst" differ; neither should have to
  start from an empty scope list.
- **Derived, not hand-written.** The grant set is computed from the
  operation registry, so it cannot drift from what the tool actually
  asks for. That is already true of `auth plan --op`, which computes a
  least-privilege union for a chosen operation set. A template is that
  call with a name, a justification and a lifetime attached.
- **Catalogued.** Templates ship as a set, versioned, reviewable, and
  distributable. sideshow already distributes content packs with
  provenance.

## Why it might matter more than it looks

**It changes who does the thinking.** Today a Wiz admin asked for a
credential decides its scopes under time pressure, which is exactly the
condition under which `read:all` wins. A catalogue moves that decision to
review time, once, for a class of requests.

**It is the artifact self-service actually needs.** `credential-plane.md`
Phase 3 imagines Information Security members provisioning their own
purpose-built credentials. They cannot pick from a catalogue that does
not exist, and the gap between "we support self-service" and "here are
the eleven roles you might be" is the entire adoption problem.

**It makes narrowing measurable.** The success metric already named is
that the live credential population narrows over time against the
`excess: 67` baseline. Templates give that metric a denominator: what
fraction of live credentials were issued from a template, and how far
does each sit from its template's grant set.

**It is a place to put the tuning knobs already discovered.** Three axes
exist (scopes, projects, expiry) and two roles can share a scope set and
differ on the others. Prose handles that badly. Data handles it.

## What is uncomfortable about it

**It is a step toward being a distro maintainer for someone else's
permission model.** The same trap `aae-orc-7128` names for pack
resolvers: curating a catalogue means owning it as the vendor changes
underneath. A tenant-derived catalogue, built from `permissionScopes` and
the tenant's own roles, might avoid that. Unknown.

**Wiz already has custom roles.** The vendor ships a custom-role builder
for USER RBAC. If the same machinery covers service accounts, a template
catalogue may be re-implementing something adjacent to a vendor feature,
which is the obsolescence test in the orc vision applied here. Worth
checking before building anything.

**A template is a claim about a job, and jobs vary.** Six templates
derived from one organisation's runbooks are six guesses about everyone
else. The tunable half is not a nice-to-have; a catalogue that cannot be
adjusted is worse than none, because it will be adopted verbatim.

**Scope assignment is still provisional.** Templates computed from
`required_scopes` inherit whatever is wrong in the registry. The
`list_issues` case is the live example: it declares `read:users` and
`read:service_accounts` because its document selects `assignee`, which
inflates three of six templates. A catalogue built today would ship that
inflation to everyone who used it.

## What would have to be true

1. Scope assignments are evidence-based rather than declared.
   `finding-009`'s denial route makes that cheap; it has not been run.
2. The three axes are settled enough to encode. Projects specifically are
   deferred and unknown (bd `aae-orc-t5cd`).
3. Someone other than us wants this. One organisation's six templates are
   a document. A catalogue is a product claim.

## Where it might go

If it crystallises: a frontier question about whether credential
templates are a stave concern, a sideshow pack, or a thing the vendor
should own. The pack framing is interesting because it inverts the usual
direction, distributing a security posture rather than agent behaviour.

If it does not: the six templates in `docs/design/` remain useful as a
document, and this file records why the generalisation was not taken.

## Cross-references

- `docs/design/role-credential-templates.md` (the six that prompted this)
- `docs/design/credential-plane.md` (Phase 3 self-service, the metric)
- `docs/runbooks/catalogue.md` (where the operation sets came from)
- `_kos/findings/finding-009-the-server-names-the-scope-it-wanted.md`
- bd `aae-orc-t5cd` (projects, deferred), `aae-orc-kuqt`, `aae-orc-oqg2`
