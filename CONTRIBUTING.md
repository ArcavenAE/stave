# Contributing

## Quick start

```sh
just setup           # install nightly rustfmt + cargo-deny
just install-hooks   # lefthook pre-commit + pre-push
just check           # mirror CI quality gates locally
```

## Workflow

- Branch from `main`. Trunk-based until distribution lands.
- Commits follow Conventional Commits. See `.claude/rules/git-commits.md`.
- All commits SSH-signed. CI rejects unsigned commits.
- Open a PR; CI runs fmt, clippy, build, test, cargo-deny, and dependency
  review.

## Sharing logs, payloads, and repros (read before filing issues)

stave runs against live Wiz tenants. Anything the tool prints can
identify a real cloud estate. Before sharing output in an issue, PR,
or commit:

1. **Prefer shapes over values.** Share `stave ops show <name>`,
   exit codes, and error text — not payloads. For response-shape
   questions, share the audit line's `shape_hash` fields, never the
   records.
2. **Reproduce against wiremock or fixtures** where possible
   (`STAVE_BASE_URL` + `examples/fixtures/`) — repros built on
   synthetic data are directly committable as failing tests.
3. **If real output is unavoidable, sanitize it**: replace the tenant
   ID with `00000000-0000-0000-0000-000000000000`; the region hostname
   with `api.<region>.app.wiz.io`; cloud account IDs with
   `123456789012`; resource names with `example-*`; names/emails with
   `example` values; delete pagination cursors and pre-signed URLs.
4. **Run repros with `STAVE_AUDIT=off`** if you plan to share your
   terminal transcript, and never share raw audit-trail lines.

The pre-commit hook (`scripts/check-tenant-leaks.sh`) blocks
tenant-shaped hostnames and key material; add your tenant's literals
to `.leak-patterns.local` (gitignored) so it also catches your tenant
ID, region, and org name.

## Schema changes

If the Wiz GraphQL schema changes upstream:

```sh
cargo xtask sync-spec         # introspect and update spec/ + sha256
cargo xtask check-ops         # validate curated operations against it
```

Open a PR with the schema bump separate from any CLI changes that
depend on it, where reasonable.

## Curated operations

`crates/stave-api/` holds curated GraphQL operation documents. Every
document must pass `cargo xtask check-ops` against the vendored
schema; never loosen the check to admit a drifted document.

## Tests

- Unit tests: `#[cfg(test)] mod tests {}` alongside source.
- HTTP-level tests: `wiremock` against the SDK.
- Snapshot tests: `insta` for stable response renderings.
- CLI smoke tests: `assert_cmd` with a built binary.
