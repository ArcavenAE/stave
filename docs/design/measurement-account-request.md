# The measurement account: exact request, for hand execution

Deliverable of bd `aae-orc-oqg2.1`. Produced entirely offline against the
vendored schema. Nothing here contacted the tenant.

Paste one of the two variants below into the Wiz console's GraphQL
explorer. Executing it once by hand produces the least-privilege
credential that the measurement work has been stalled on.

> **The response contains a client secret.** `ServiceAccount.clientSecret`
> is `String!` and is returned exactly once. Do not paste the response
> into a transcript, an issue, a commit, a chat, or a file. Read it in the
> browser and put it straight into the keyring by the procedure in
> "After it exists" below. This is why `DENIED_SELECTIONS` blocks that
> field in our own binary: the field is legitimate here and nowhere else.

---

## Why this account, and what it unblocks

The development credential holds 79 scopes including `read:all`, against
a declared requirement of twelve. Under `read:all`, a check that a given
`read:*` scope is sufficient is vacuous, because every `read:*` scope is
satisfied whether or not the tenant recognises its name. That is the
whole reason the following are stuck rather than merely unfinished:

- `aae-orc-cw9y`, scope-qualification study. Its stated blocker is this credential.
- `aae-orc-vhje`, grant vocabulary. The comparison route is dead; this is the live route.
- `aae-orc-rsh6`, whether `cloudResourcesV2` works under `read:resources`.
- `aae-orc-i8cj`, the silent-scope-stripping discriminator, which needs variant B below.
- `SCOPE_METADATA_PROVISIONAL`, true since scaffold.

## The scope names are validated

Measured 2026-08-08 against the caller's own `ServiceAccount.scopes` read
through the tenant directory, testing **literal** set membership with no
`read:all` implication applied. All twelve declared names are literally
present in the granted set of 79.

This supersedes the position in `aae-orc-h9uo`, which held that only
`admin:audit` and `read:all` were validated. That ticket's critique of
finding-007's *reasoning* stands and should still be fixed: `missing: 0`
could not have carried the conclusion, because it ran through
`scope_granted`, which treats `read:all` as satisfying any `read:*`. The
conclusion happens to be true, on evidence finding-007 did not have.

Probed in the same call: **both `read:users` and `read:user_accounts` are
real and both are granted.** Neither is a typo. So the open question is
not spelling, it is which of two real scopes governs `list_users`, and
the registry assigns `read:users` without evidence. That is a
per-operation assignment question, which is what
`SCOPE_METADATA_PROVISIONAL` covers.

**Pre-registered prediction.** Variant A grants `read:users` and not
`read:user_accounts`, deliberately. If `list_users` fails or returns
empty under the measurement credential, the registry's assignment for
that operation is wrong and `read:user_accounts` is the correct one.
Recording it here so the outcome reads as a test result rather than a
post-hoc explanation. Granting both would be the wrong fix: it would
restore the ambiguity the least-privilege credential exists to remove.

---

## The mutation document

Identical for both variants. Only the variables differ.

```graphql
mutation CreateMeasurementServiceAccount($input: CreateServiceAccountInput!) {
  createServiceAccount(input: $input) {
    serviceAccount {
      id
      name
      clientId
      clientSecret
      scopes
      expiresAt
      enabled
      type
    }
  }
}
```

Every field above exists on `ServiceAccount` in the vendored schema
(`spec/`, lines 85757 to 85780). `scopes` and `expiresAt` are selected so
the response itself is the verification that the grant landed as asked.

### Variant A: the twelve-scope measurement account

Substitute `<owner>` with the requesting human's handle in both `name`
and `description`. Everything else is literal.

```json
{
  "input": {
    "name": "stave-measurement-<owner>-202608",
    "description": "Least-privilege measurement credential for stave. Requested by <owner>. Justifying ticket: aae-orc-oqg2.1. Grants exactly the twelve scopes stave's operation registry declares. Deliberately excludes read:all.",
    "type": "CLI",
    "expiresAt": "2026-09-07T00:00:00Z",
    "scopes": [
      "admin:audit",
      "read:cloud_accounts",
      "read:cloud_configuration",
      "read:controls",
      "read:issues",
      "read:projects",
      "read:reports",
      "read:resources",
      "read:security_frameworks",
      "read:service_accounts",
      "read:users",
      "read:vulnerabilities"
    ]
  }
}
```

### Variant B: the `aae-orc-i8cj` discriminator

The same eleven, with `read:projects` withheld. Nothing else changes.

```json
{
  "input": {
    "name": "stave-i8cj-discriminator-<owner>-202608",
    "description": "Discriminator credential for stave. Requested by <owner>. Justifying ticket: aae-orc-i8cj. Variant A minus read:projects, to determine whether a withheld grant makes fields vanish silently or errors.",
    "type": "CLI",
    "expiresAt": "2026-09-07T00:00:00Z",
    "scopes": [
      "admin:audit",
      "read:cloud_accounts",
      "read:cloud_configuration",
      "read:controls",
      "read:issues",
      "read:reports",
      "read:resources",
      "read:security_frameworks",
      "read:service_accounts",
      "read:users",
      "read:vulnerabilities"
    ]
  }
}
```

**What variant B decides.** Run an operation declaring `read:projects`
under this credential. If project fields come back null or absent while
the call succeeds, silent scope stripping is confirmed and a P1 closes,
because it means every field-population measurement we have taken is
confounded by grants rather than by data. If the call errors, the
hypothesis dies cleanly and field-population results mean what they say.
The over-privileged credential can produce neither outcome, which is why
this has been undecidable rather than merely untested.

---

## Field rationale, including the two guesses

| Field | Value | Confidence |
|---|---|---|
| `name` | `stave-<purpose>-<owner>-<yyyymm>` | Convention from `credential-plane.md`. Makes `list_service_accounts` a self-describing inventory. |
| `description` | requester plus justifying ticket | Convention. This is the purpose record that makes narrowing auditable later. |
| `expiresAt` | `2026-09-07T00:00:00Z` | Chosen, not derived. 30 days. Re-minting is cheap, so short is the right default; lengthen deliberately if the study runs long. Never unbounded. |
| `type` | `CLI` | **A guess.** `ServiceAccountType` offers `CLI` and `INTEGRATION` among others, and `INTEGRATION` is what the console's integration flow produces. `CLI` is semantically right for a credential a CLI holds. Settle it by widening `list_service_accounts` to select `type` and reading back what the existing hand-made accounts are. |
| `scopes` | the twelve, or eleven | Literally validated today. |
| `assignedProjectIds` | omitted | Deliberate. Variant B tests project-scope behaviour through the scope axis; narrowing by project at the same time would confound the two. Add it for issued credentials in Phase 3. |

`type` is the only input where a wrong value has a real cost, since it may
determine which console surfaces the account appears in. It is optional in
the schema, so omitting it and letting the server default is a legitimate
alternative if the guess proves wrong.

---

## After it exists

1. **Enrol it without the secret ever touching disk outside the keyring.**

   ```
   stave profile add measurement --client-id <clientId from the response> \
     --purpose "twelve-scope measurement credential"
   stave auth login --profile measurement
   ```

   The client ID is not secret in the same way, but it is still hygiene
   class 4, so it does not go into a commit or an issue either. The secret
   goes only into the hidden prompt.

2. **Verify the grant landed, rather than trusting the request.**

   ```
   stave --profile measurement auth scopes --from-directory \
     | scripts/scope-membership.sh
   ```

   Expected: granted set size 12, `read:all literally present: no`, twelve
   `OK` lines, exit 0. Any other result means the tenant did something to
   the request, and the measurement work should stop until it is
   understood. The script derives the declared set from `required_scopes`
   in the registry rather than restating it, so it cannot drift from what
   stave actually asks for.

   Run the same check against the development credential for contrast: it
   reports 79 and `read:all literally present: YES`, which is the
   `excess: 67` baseline this account is meant to replace.

3. Only then resume `cw9y`, `rsh6` and `vhje`, and run the variant B
   comparison for `i8cj`.

---

## Cross-references

- `docs/design/credential-plane.md` (the plane split, phases, and why this is Phase 0)
- `aae-orc-oqg2` and `aae-orc-oqg2.2`; `aae-orc-h9uo` (the finding-007 correction this partly supersedes)
- `docs/design/read-only-posture-and-permissions-report.md` (D1, D3, and `scope_granted`)
- `charter.md` F1 (`SCOPE_METADATA_PROVISIONAL` and why it stays true)
