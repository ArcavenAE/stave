---
name: tenant-leak-scan
description: "Use before any stave output, error text, audit line, or draft leaves the terminal: a commit, an issue or PR body, a bug report, a pasted transcript, a fixture, a doc example, or a finding. Scans text for tenant-identifying data (cloud account and resource identifiers, posture findings tied to real resources, credentials, PII, region-bearing hostnames), reports what it found by class without echoing the value, and gives the sanitized form to use instead."
version: "1.0"
provenance: "Filed 2026-08-06 after an agent printed a person's name, a GCP service-account email, and an OCI compartment OCID into a session transcript during the first live demo attempt against the production tenant. The commit hook would not have fired, because no commit was involved. The scrubber existed and was not in the pipe."
---

# Tenant Leak Scan

stave's subject matter is a live Wiz tenant watching real cloud estates.
Almost every payload, and much of the metadata, describes real accounts,
real resources, and their unremediated weaknesses. Publishing that is not
a style problem. It is handing out a targeting map.

This skill is the check you run on text that is about to leave the
terminal. `.claude/rules/tenant-data-hygiene.md` is the always-loaded
behavior trigger; this is the procedure it hands off to.

## When this fires

- Drafting a commit message, issue, PR body, comment, or discussion post
  in `stave`, or anywhere describing stave behavior
- Pasting stave stdout, stderr, or an audit line anywhere outside your
  own terminal
- Writing a kos finding, node, idea, or charter edit from a live run
- Adding or regenerating anything in `examples/fixtures/`
- Writing a test whose expected value came from a real response
- Handing output to another agent, tool, or model

## Procedure

### 1. Scrub first, then look

Do not read the raw text and judge it by eye. Names, bucket names, and
project slugs have no shape and read as ordinary words.

```sh
scripts/scrub.sh < draft.txt                 # text or stave JSONL, auto-detected
stave list issue --limit 50 | scripts/scrub.sh
scripts/scrub.sh --catalog < controls.jsonl  # keeps vendor catalog names
```

For stave JSONL the scrubber uses a **default-deny field allowlist**: only
fields known to be non-identifying survive. A field the vendor added
yesterday is redacted, not leaked.

### 2. Scan what you are about to publish

```sh
scripts/check-tenant-leaks.sh --staged   # what is about to be committed
scripts/check-tenant-leaks.sh --all      # the whole tree
```

The detector enforces the `block` tier only. Its silence is not a
clearance: it does not know a bare bucket name, a project slug, or a
person's name. Step 3 is not optional.

### 3. Read against the classes by hand

The scanners cover shapes. You cover meaning. For each item, ask whether
the text names one:

| Class | Examples | Verdict |
|---|---|---|
| Tenant identity | tenant GUID, `api.<real-region>.app.wiz.io`, registry username, org name, org-named projects | never |
| Cloud estate identity | account and subscription ids, ARNs, OCIDs, resource and bucket and cluster names, IPs, hostnames | never |
| Posture tied to the estate | issue records, vuln-to-resource mappings, attack paths, "this bucket is public" naming the bucket | never |
| Credentials | client ids and secrets, tokens, registry passwords, pre-signed report URLs | never |
| PII | user names, emails, identity-provider records | never |
| Raw audit lines | carry query variables, cursors, local hostname and username, CEL text | never |
| Abstract vocabulary | a CVE id, a Wiz control name, a severity, a status, a count, a timestamp | fine |

The line is **abstract versus attached**. `CRITICAL`, `TOXIC_COMBINATION`,
`CVE-2024-1234`, and "11 accounts unscanned past 30 days" are all fine.
The same severity attached to a named resource is a targeting map.

### 4. Prefer a shape over a value

Most of the time the identifier was never the point:

- Reproduce against `examples/fixtures/` (synthetic) and wiremock. A repro
  on synthetic data is directly committable as a failing test.
- Quote the operation name, the exit code, the error text, the field name,
  the record count. Not the record.
- Aggregate. Counts and distributions carry the finding; rows carry the
  leak.
- Run shareable repros with `STAVE_AUDIT=off`.

If a real value is unavoidable, substitute:
tenant id to `00000000-0000-0000-0000-000000000000`, region to `<region>`,
account ids to `123456789012`, resources to `example-*`. Delete pagination
cursors and pre-signed URLs outright.

### 5. If something already escaped

1. Do not re-quote the value while reporting it, including in the fix.
2. If it was committed but not pushed, the content must not enter history.
   Stop and tell the user; history rewriting is theirs to authorize, never
   an agent's call.
3. If it was pushed or posted, tell the user immediately and plainly, with
   the class and the location. Treat pushed as public: deletion is not
   containment, so credentials get rotated rather than edited away.
4. Add the missing pattern or field to `scripts/leak-patterns.sh` or the
   `scrub.sh` allowlist, with a selftest case that fails without the fix.
5. File it per `.claude/rules/tooling-friction.md` if a tool made the leak
   easy.

## Extending the scanners

`scripts/leak-patterns.sh` is the one source both halves read.

- Add a rule as `<PCRE>\t<replacement>\t<tier>`.
- `scrub` is the default tier for anything new. It neutralises output and
  never blocks a commit.
- Promote to `block` only after measuring zero benign hits across the
  tree. A checker that cries wolf gets disabled within a week, and then
  nothing is checked at all.
- Add a `scrub.sh --selftest` case in the same commit, with a synthetic
  value. A rule with no test is a rule that silently stops matching.
- For a new stave field, decide allowlist membership rather than writing a
  pattern. Default-deny means doing nothing is already the safe outcome.

## Known limits

State these rather than implying coverage:

- The field allowlist covers stave's JSONL. Free text (error messages,
  CEL predicate text, GraphQL variables) gets only the structural tier.
- The structural tier is shapes. It cannot recognize a name, a bucket, or
  a slug.
- `.leak-patterns.local` is per-machine and gitignored. A machine without
  one gets the structural tier only, which is by design: the literals must
  never enter git.
- Field-level payload redaction inside the SDK is still an open item
  (`crates/stave-sdk/src/redact.rs` covers headers and argv only). Until
  it lands, the scrubber is a wrapper the caller must remember, which is
  exactly the failure mode that produced this skill.

## Cross-references

- `.claude/rules/tenant-data-hygiene.md` (the always-loaded trigger)
- `scripts/scrub.sh`, `scripts/check-tenant-leaks.sh`,
  `scripts/leak-patterns.sh`
- SECURITY.md § Tenant Data Hygiene, CONTRIBUTING.md § Sharing logs
- orc `.claude/rules/upstream-claim-gate.md` for the publishing sibling:
  that gate asks whether a claim is true, this one asks whether it is
  safe to say. Both run before the same keystroke.
