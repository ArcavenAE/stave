# CLAUDE.md — stave

Rust CLI for the Wiz GraphQL API. Unofficial; not affiliated with or
endorsed by Wiz, Inc. Vendored GraphQL schema as the contract,
audit-trail-as-feature, agent-first ergonomics, read-only against the
live tenant by default. Third sibling of sidestep and bloomctl.

> **⚠ Tenant data hygiene (load-bearing).** stave operates against a
> live Wiz tenant watching real cloud estates. NEVER put the tenant ID,
> the region-bearing API hostname, cloud account/resource identifiers,
> issue/vulnerability records tied to real resources,
> credentials/secrets (client secrets, tokens, registry passwords), or
> raw audit-trail lines into git commits, `gh` issues/PRs, discussions,
> or any shared log. Sanitize first.
> Full rule: `.claude/rules/tenant-data-hygiene.md`.

@charter.md
@.claude/rules/_index.md

## Build / Run / Test

Requires: Rust 1.85+ (Edition 2024), `just`, nightly rustfmt.

```sh
just build              # cargo build --workspace
just test               # cargo test --workspace
just check              # fmt-check + clippy + cargo-deny
just run -- --version   # invoke the CLI
just sync-spec          # cargo xtask sync-spec — refresh vendored GraphQL schema
```

## Architecture

```
crates/
  stave-api/         Curated GraphQL operations (documents + registry metadata)
  stave-sdk/         Hand-written: OAuth token flow, resolution chains, audit,
                     redaction, write-guard, GraphQL execution, primitives
  stave-cli/         clap CLI: auth/config/ops/api + list/get/search/
                     filter/enrich/emit
  stave-mcp/         Placeholder — MCP *server* backed by stave-sdk

xtask/               cargo xtask sync-spec | check-ops
spec/                Vendored GraphQL schema (+ sha256 pin)
docs/                Audit-trail format, design notes
examples/            Wiz fixtures (synthetic), jq asserts, recipes
```

Three-layer call graph: `cli/mcp → sdk → api`. Audit emission,
redaction, the write-guard, and the resolution chains live in the SDK
so every consumer inherits them.

## Conventions

- **Language:** Rust, edition 2024, MSRV 1.85.
- **No unsafe:** `#![forbid(unsafe_code)]` everywhere.
- **Auth:** OAuth2 client-credentials. client_id via flag → env
  (`STAVE_CLIENT_ID`) → config; client_secret via env
  (`STAVE_CLIENT_SECRET`) → keyring → config; api_url via flag → env
  (`STAVE_API_URL`) → config → derived from the token's data-center
  claim. One chain shape, per cli-philosophy.md. Minted tokens are
  cached in the XDG state dir, never in config.
- **Write-guard:** GraphQL mutations refuse without `--allow-write` /
  `STAVE_ALLOW_WRITE` / config opt-in. The live tenant is production —
  keep it that way.
- **Audit trail:** every API call emits a JSONL line under the XDG
  state dir. See `docs/audit-trail-format.md`.
- **No file deletion:** never delete user files. Overwrite only with explicit intent.
- **Git workflow:** trunk-based on `main`.

## How to Work Here (kos Process)

### Re-introduction
Read charter.md before any substantive work.

### Session Protocol
1. Read charter.md (orient)
2. Identify the highest-value open question — or capture ideas in `_kos/ideas/`
3. Write an Exploration Brief in `_kos/probes/`
4. Do the probe work
5. Write a finding in `_kos/findings/`
6. Harvest: update affected NODES (`_kos/nodes/{bedrock,frontier,graveyard}/*.yaml`),
   move files if confidence changed. Keep charter edits light per orc
   `.claude/rules/charter-light-touch.md`.

Cross-repo questions belong in the orchestrator's `_kos/`.
