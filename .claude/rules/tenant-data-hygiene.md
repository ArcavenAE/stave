# Tenant Data Hygiene — Never Publish Cloud-Posture Data

Behavior-trigger rule. Same shape as the global force-push abort signal
and orc `tooling-friction.md`. Names the *behavior*, not the concept,
because concepts are easy to rationalize past at the keystroke.

stave's entire subject matter is a live Wiz tenant watching real cloud
estates. Every payload, and most metadata, describes real cloud
accounts, their resources, and their unremediated weaknesses. A leak
here is not a style problem — it is publishing a targeting map of a
production environment to anyone who reads a public repo.

## Trigger 1 (the earlier one): you are about to READ tenant output

Before running any `stave` command whose output lands in a terminal, a
transcript, a chat window, an issue draft, or an agent's context:

**pipe it through `scripts/scrub.sh`.**

```sh
stave list issue --limit 50 | scripts/scrub.sh
stave list control --limit 50 | scripts/scrub.sh --catalog   # keeps vendor names
stave api --query "$doc" 2>&1 | scripts/scrub.sh             # errors leak too
```

The trigger fires on the *pipeline you are composing*, not on the file
you are saving. If the command produces tenant records and you have not
typed `| scripts/scrub.sh`, stop and add it.

Applies equally to: `jq` over stave output, `head`/`tail` of an audit
line, `cat` of a saved response, and any `--limit 1` you tell yourself
is "just to check the shape." One record is one real person or one real
resource.

Read raw only when you have a specific reason to see an identifier, and
then keep it inside the terminal: no file, no commit, no paste, no
issue body.

### Why this trigger comes first

Trigger 2 below is enforced by a pre-commit hook and CI. This one has
**no backstop at all**. Once tenant data is in a transcript it has
already left the machine's control, and there is nothing left to block.
No hook runs on your eyes, your scrollback, or a model's context
window.

Measured 2026-08-06, during the first live demo attempt: an agent ran
`stave list issue --limit 3 | stave enrich --with severity-roll-up`
with no scrubber and printed a person's full name, a GCP project and
service-account email, and an OCI compartment OCID into a session
transcript. The scrubber for that session existed, in a scratch
directory, and was simply not in the pipe. Nothing was committed, and
the commit hook would not have fired if it had been, because no commit
was involved. That is the whole gap in one sentence.

Note what the regex tier could not have caught: the person's name. It
has no shape. `scrub.sh` catches it because `entitySnapshot.name` is
absent from a default-deny field allowlist, not because it looked
dangerous. This is why the answer is the scrubber and not a sharper
pattern.

## Trigger 2: you are about to WRITE to a durable channel

You are about to put text into a **durable, shareable channel** — a git
commit message, a file being committed, a `gh issue`/`gh pr` body or
comment, a PR description, a discussion post, a published artifact, or
any log/transcript you paste outward. STOP and scan that text for the
forbidden classes below before it lands.

Also stop when you're about to:
- Paste live `stave` output (stdout OR an audit line) into any of
  the above "to illustrate the bug/feature."
- Regenerate a fixture in `examples/fixtures/` from a real API response.
- Write a test whose expected value is a real resource id / account id
  / subscription id / hostname.
- Include a real `--vars`/`--where` value in an example or doc.

## Forbidden in any durable/shareable channel — NEVER

1. **Tenant identity** — the tenant ID (GUID), the region-bearing API
   hostname (`api.<region>.app.wiz.io` with a real region), the
   registry username (it embeds the tenant ID), the org name, project
   names that name the org.
2. **Cloud estate identity** — cloud account IDs, subscription IDs,
   ARNs, resource names/IDs, VPC/cluster/bucket names, IP addresses,
   hostnames, external IDs of connectors.
3. **Posture and findings tied to the estate** — issue records,
   vulnerability-to-resource mappings, attack-path/graph output,
   misconfiguration details naming real resources. (A CVE or rule ID
   in the abstract is fine; "this bucket is public" with a real name
   is a targeting map.)
4. **Credentials & secrets** — service-account client IDs and secrets,
   OAuth tokens, registry passwords, report download URLs (they are
   pre-signed), anything from secrets-shaped fields in responses.
5. **PII** — user names, emails, identity-provider records from
   users/serviceAccounts queries.
6. **Raw audit-trail lines** — they carry real query variables,
   pagination cursors (which can embed tenant data), local
   hostname/username, and CEL predicate text.

## The fix — sanitize before the keystroke

- Prefer **shapes over values**: operation names, exit codes, error
  text, `shape_hash` — not payloads.
- Reproduce against **wiremock + `examples/fixtures/`** (synthetic).
  A repro on synthetic data is directly committable as a failing test.
- If real output is unavoidable, substitute: tenant ID →
  `00000000-0000-0000-0000-000000000000`; region → `<region>`; account
  IDs → `123456789012`; resource names → `example-*`; delete pagination
  cursors and pre-signed URLs.
- Run shareable repros with `STAVE_AUDIT=off`.

## Backstops (defense in depth — not a substitute for the rule)

- `scripts/scrub.sh` is the TRANSFORM (trigger 1). Field allowlist over
  stave JSONL plus structural patterns over any text.
  `scripts/scrub.sh --selftest` proves every rule fires, using
  synthetic values only, and needs no tenant.
- `scripts/check-tenant-leaks.sh` is the DETECTOR (trigger 2), run by
  pre-commit (lefthook) and CI.
- Both read `scripts/leak-patterns.sh`, so a pattern added once is
  scrubbed and blocked in the same commit. Rules are tiered: `block`
  shapes have zero benign occurrences in the tree and stop a commit;
  `scrub` shapes (GUID, bare account id, IP, email) are neutralised in
  output but never block, because they occur legitimately in synthetic
  fixtures and tests. Promote a rule to `block` only after measuring
  zero benign hits.
- Tenant literals (the tenant ID, the region hostname) go in a
  **gitignored** `.leak-patterns.local`, one fixed string per line.
  Create one on every machine that touches a real tenant; the scrubber
  and the detector both pick it up automatically.
- GitHub secret scanning + push protection are enabled on the repo.
- Fixtures are synthetic **by policy** (`examples/README.md`).

The hooks catch hostnames and key material. They do NOT catch a bare
account ID, a bucket name, or an email — those are on you at the
keystroke. That's why this is a behavior rule, not just a scanner.

## Why this rule exists

The write-guard makes stave safe to *run* against production. This
rule makes stave safe to *develop in the open*. Both are required:
a tool that never mutates the tenant but leaks its security posture to
a public issue tracker has failed the same user. See SECURITY.md
"Tenant Data Hygiene", CONTRIBUTING.md "Sharing logs, payloads, and
repros", and the sanitization checklist baked into the issue templates.

## Cross-references

- Behavior-trigger pattern: `~/.claude/CLAUDE.md` (force-push abort),
  orc `tooling-friction.md`
- SECURITY.md § Tenant Data Hygiene · CONTRIBUTING.md § Sharing logs
- `.github/ISSUE_TEMPLATE/` — the checkbox checklist enforces this on
  every issue
