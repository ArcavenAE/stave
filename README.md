# stave

An unofficial CLI for the Wiz API. Not affiliated with or endorsed by
Wiz, Inc.

Rust CLI for the Wiz GraphQL API, designed for LLM-driven workflows,
with a local audit trail intended to be mined for the verbs and nouns a
v0.2 surface should curate. Third sibling of
[sidestep](https://github.com/ArcavenAE/sidestep) and
[bloomctl](https://github.com/ArcavenAE/bloomctl) — same pattern,
different vendor.

The name stacks three things in one syllable: the object (a staff), the
inscription (rune-stave), and the verb (stave off). The same shape from
three directions.

> Status: scaffold. Auth (env / keyring / config), GraphQL operation
> dispatch, the primitive verbs, the write-guard, and the audit trail
> are wired and tested against a mock server.

## Why stave

- **Schema-driven.** The Wiz GraphQL schema is vendored under `spec/`
  and pins the surface stave talks to. Curated operations are checked
  against it; `stave api` runs any query document.
- **SDK-backed.** The same SDK powers the CLI and future MCP surfaces.
  Auth, audit, redaction, and the write-guard live in one place.
- **Agent-first.** JSONL output for non-TTY, predictable verb shape,
  stable operation names, structured audit trail, CEL predicates.
- **Read-only, unconditionally.** The tenant is production. Every
  GraphQL mutation and subscription is refused, with no flag, env var,
  or config key to lift it. Ad-hoc `--query` documents run only under
  the exploratory read posture (`stave config set posture
  exploratory`); the default curated posture refuses them. The real
  boundary is a read-only service account — `stave auth plan` prints
  the least-privilege scopes to provision one, and the scopes to
  withhold. This client-side guard is operational friction, not a
  security control (see SECURITY.md).
- **Audit as feature.** Every API call emits a JSONL line locally; a
  future pass mines those traces to propose the composite verbs Wiz
  workflows actually need.

## Install

```sh
brew tap ArcavenAE/tap                        # one-time
brew install ArcavenAE/tap/stave
```

## Bootstrap

Create a Wiz service account (Settings → Access Management → Service
Accounts) with the read scopes you need, then:

```sh
stave auth login          # prompts for client ID + secret; secret goes to the platform keyring
stave auth status         # verifies a token can be minted; reports every source
```

Non-interactive:

```sh
printf '%s' "$WIZ_CLIENT_SECRET" | stave auth login --client-id <id> --stdin
```

Your tenant's API endpoint is shown in the Wiz portal (user profile →
tenant info). Persist it once:

```sh
stave config set api_url https://api.<region>.app.wiz.io/graphql
```

## Use

```sh
stave list issue --limit 20                      # JSONL stream, _kind-tagged
stave list vulnerability_finding --limit 50
stave get project <id>
stave list issue | stave filter --where 'severity == "CRITICAL"' | stave emit --format md
stave api --query ./my-query.graphql --vars '{"first": 5}'   # schema-checked escape hatch
```

## Trademarks

Wiz is a trademark of Wiz, Inc. The name is used here only nominatively
and factually, to describe what this tool talks to. This project is not
affiliated with, sponsored by, or endorsed by Wiz, Inc.

## License

MIT
