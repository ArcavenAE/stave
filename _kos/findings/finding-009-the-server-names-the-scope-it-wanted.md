# finding-009: the server names the scope it wanted

2026-08-08, bd `aae-orc-kuqt`, with consequences for `aae-orc-cw9y`,
`aae-orc-vhje`, `aae-orc-i8cj` and `SCOPE_METADATA_PROVISIONAL`. One
live call, read-only, coach-cleared, and it failed. The failure is the
finding.

## What was run

`list_permission_scopes` was curated to read the tenant's scope
vocabulary, as distinct from what any one account is granted. Its
`required_scopes` had to be declared and nothing derives it: the vendored
schema carries no scope directives at all, zero hits for `@auth`,
`@requiresScope` and `@scope`. So `read:permission_scopes` was declared
as a guess from the tenant's observed naming convention,
`read:<resource_plural>`, and marked in the source as the weakest
declaration in the registry.

It was declared deliberately as a scope the twelve-scope `measurement`
credential does not hold. `required_scopes` never gates a read; it feeds
`ops permissions`, `auth can-i` and the `auth plan` grant union only. So
the first run was a test of the declaration, pre-registered in the
registry comment before it ran.

    stave --profile measurement list permission_scope --limit 100

## What came back

    stave: GraphQL: access denied, at least one of the following is
    required: [read:all read:permission_scopes], your permissions:
    [admin:audit read:cloud_accounts read:cloud_configuration
     read:controls read:issues read:projects read:reports read:resources
     read:security_frameworks read:service_accounts read:users
     read:vulnerabilities]

## Four results, in ascending order of importance

**The guess was right.** `read:permission_scopes` is the real name. The
convention held. That is the smallest of the four.

**The credential is confirmed from the server side.** Twelve scopes, no
`read:all`, matching what was minted and what
`scripts/scope-membership.sh` read from the directory. Two independent
routes now agree.

**`read:all` is an implication scope, on the vendor's own word.** It
appears as an alternative to the specific scope in the server's list.
D3's `read:all` rule in `scope_granted` has been unvalidated since
scaffold and marked provisional; this is the first direct evidence for
it. Scoped honestly: evidence for one operation, not a general proof
that `read:all` implies every `read:*`.

**The server publishes required scopes in its denials.** This is the one
that matters, because it is a method rather than a fact.

## The method nobody planned

Per-operation scope assignment has been the open half of
`SCOPE_METADATA_PROVISIONAL` since scaffold, and every route designed for
it was expensive. finding-008 named the clean discriminator as variant
B's shape applied per scope: twelve accounts, each holding the twelve
minus the one under test, and it was judged more minting than the
question was worth. The plan in `credential-plane.md` treats scope
assignment as something to be inferred by elimination.

None of that is necessary. A credential that lacks a scope makes the
server say what it wanted. So:

> **To learn an operation's required scopes, call it with a credential
> that cannot perform it, and read the refusal.**

One deliberately under-privileged credential reveals the requirements of
every operation it cannot perform, at one call each. A credential holding
nothing, or holding a single unrelated scope, would map the entire
registry in fourteen calls.

This also inverts a habit. Every probe in this repo so far has been
designed around making a call succeed, and the denial path was treated as
the failure case to be avoided. Here the denial carries strictly more
information than the success would have: a successful read returns the
catalogue but says nothing about what authorised it, which is exactly the
gap finding-008 recorded for `read:users` and `list_users`.

## What it does not license

The axis is one operation, one credential, one denial. Applying
corollary 1 of [[elem-validation-scope-matches-claim-scope]]:

- It does not show that every operation's denial names its scopes. One
  root behaved this way. The registry has fourteen.
- It does not validate the other thirteen `required_scopes` declarations.
  It supplies the method for validating them; that is not the same thing
  and the distinction is the whole subject of finding-008.
- It does not show `read:all` implies every `read:*` scope. It shows the
  server offered `read:all` as an alternative for this one operation.
- The echoed permission list is the server's view of this credential, not
  a general statement that denials echo grants.

## What follows

- The registry comment for `list_permission_scopes` is corrected from
  guess to server-validated, with the refusal quoted.
- Reading the catalogue itself still needs a credential holding
  `read:permission_scopes`, which none of ours does. It is one scope on
  the next mint rather than a new account.
- The scope-discovery sweep is now cheap enough to be worth doing
  properly, and it should be pre-registered before it runs, because a
  denial-driven method has an obvious failure mode: an operation that
  fails for some other reason produces an error that is not a scope
  statement, and reading it as one would manufacture assignments.

## Cross-references

- `_kos/findings/finding-008-twelve-names-real-on-evidence-the-claim-never-had.md`
- `_kos/nodes/bedrock/elem-validation-scope-matches-claim-scope.yaml`
- `docs/design/read-only-posture-and-permissions-report.md` (D3 and `scope_granted`)
- `docs/design/measurement-account-request.md`, `docs/design/credential-plane.md`
- bd `aae-orc-kuqt`, `aae-orc-cw9y`, `aae-orc-vhje`, `aae-orc-h9uo`
