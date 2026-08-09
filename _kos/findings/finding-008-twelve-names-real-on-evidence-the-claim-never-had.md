# finding-008: the twelve names are real, on evidence the claim never had

2026-08-08, bd `aae-orc-oqg2.1` and `aae-orc-h9uo`. One live call,
read-only, coach-cleared. Third instance of
[[elem-validation-scope-matches-claim-scope]], and the first where the
criticised claim survives the correction.

## What was claimed, and on what

finding-007 concluded that the twelve scope names the operation registry
declares "are now real strings this server recognises", resting on
`missing: 0` from `auth plan --check --from-directory`.

`aae-orc-h9uo` showed that number could not carry the claim. Both
`auth can-i` and `auth plan --check` filter through `scope_granted`,
which treats `read:all` as satisfying any `read:*` requirement. The
development credential holds `read:all`. Eleven of the twelve are
`read:*`. So `missing: 0` was equally consistent with the tenant
recognising none of those eleven names. h9uo concluded that exactly two
names were validated, `admin:audit` (not `read:*`, so no implication
reached it) and `read:all` itself (membership tested directly).

That reasoning is correct and it is not retracted here.

## The test that ranges over the claim

The claim is about literal names. So the test has to be literal
membership, with no implication applied anywhere:

    stave --profile reader auth scopes --from-directory \
      | scripts/scope-membership.sh

`scope-membership.sh` reads the caller's own `ServiceAccount.scopes`
array and does `grep -qxF` per declared name. It does not consult
`scope_granted`, so `read:all` has no privileged status in it. It
derives the declared set from `required_scopes` in the registry rather
than restating it, so it cannot drift from what stave actually asks for,
and it prints only per-scope verdicts rather than the 79 entries of
privilege posture.

**Result: all twelve are literally present in the granted set of 79.**

So the validated count is twelve, not two. h9uo's critique of the
inference stands; its conclusion about the count does not survive a test
that actually ranges over the claim.

## The distinction worth keeping

finding-007 was right, and it was right by luck. It asserted something
true using a measurement that could not have detected it being false.
Those are different states and the difference is not academic: had one of
the eleven been misspelled, nothing in the pipeline would have said so,
and the misspelling would have ridden into a `createServiceAccount`
request for hand execution against a production tenant.

The general shape, already bedrock: a validation licenses only claims
inside the scope it actually ran. What this instance adds is that a
**true** claim can sit outside that scope too. The two prior instances
both had a false or overbroad claim to point at, which made the gap
findable by noticing a wrong answer. Here every answer was right. The gap
was findable only by asking what the measurement ranged over.

## Two spellings, both real

Probed in the same call, on the operator's correction that the user
scope is `read:user_accounts`:

    PRESENT  read:user_accounts
    PRESENT  read:users

Both exist and both are granted. This is not a typo on either side. Our
registry assigns `read:users` to `list_users` and `list_projects`, and
that assignment has no evidence behind it, which is exactly the territory
`SCOPE_METADATA_PROVISIONAL` covers. Twelve names being real says nothing
about which operation needs which one.

Two service-account creation scopes were probed at the same time and are
not granted to the read credential, as expected. Absence proves nothing
about existence: the granted set is what this account holds, not the
tenant's vocabulary. Only the `permissionScopes` catalogue settles
existence, which is bd `aae-orc-kuqt`.

## Pre-registered prediction

Variant A of the measurement-account request grants `read:users` and not
`read:user_accounts`, deliberately. If `list_users` fails or returns
empty under that credential, the registry's assignment for that operation
is wrong and `read:user_accounts` is the correct one. Recording it now so
the outcome is a test result rather than a post-hoc explanation.

## What changed

- `SCOPE_METADATA_PROVISIONAL` stays `true`. Names validated, assignment
  not. Unchanged by this finding.
- Charter F1 already reads "Twelve scope NAMES are validated; the
  per-operation ASSIGNMENT is not." That sentence was unsupported when
  written and is supported now. No charter edit is required, which is the
  correct outcome under `charter-light-touch.md`.
- `aae-orc-h9uo` stays open. The shippable defect it names is untouched:
  `can-i` and `plan --check` still emit `allowed: true` with no
  indication the verdict rests on implication rather than a literal
  grant. Under `cli-philosophy.md` that is a map omitting its own next
  step. Now that the literal test exists, the fix has an obvious shape:
  disclose the satisfying route.

## Cross-references

- `docs/design/measurement-account-request.md` (the request this
  unblocked), `docs/design/credential-plane.md`
- `scripts/scope-membership.sh`
- `_kos/findings/finding-007-a-second-route-to-our-own-scopes.md`
- `_kos/nodes/bedrock/elem-validation-scope-matches-claim-scope.yaml`
- bd `aae-orc-oqg2.1` (closed), `aae-orc-h9uo`, `aae-orc-kuqt`
