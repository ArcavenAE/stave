# stave — unofficial Rust CLI for the Wiz API

default:
    @just --list

# ─── Build & Run ───────────────────────────────────────

build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

run *args:
    cargo run --bin stave -- {{args}}

# ─── Test ──────────────────────────────────────────────

test:
    cargo test --workspace --all-targets

test-doc:
    cargo test --workspace --doc

# ─── Quality Checks ────────────────────────────────────

check: check-fmt check-clippy check-deny

check-fmt:
    cargo +nightly fmt --all -- --check

check-clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

check-deny:
    cargo deny check advisories licenses bans

lint: check

# ─── Formatting ────────────────────────────────────────

fmt:
    cargo +nightly fmt --all

# ─── CI Mirror ─────────────────────────────────────────

ci: check-fmt check-clippy build check-deny test test-doc check-ops hygiene

# ─── Tenant Hygiene ────────────────────────────────────

# Both halves of the leak tooling, plus the run harness that composes
# with them. Synthetic values only; no tenant, no credentials, no
# network, and the stave binary is never invoked.
hygiene: scrub-selftest runlog-selftest judge-selftest check-leaks

# Prove every scrub rule still fires, and that safe fields survive.
scrub-selftest:
    scripts/scrub.sh --selftest

# Prove the run harness scrubs by construction and refuses to execute an
# invocation with no matching CLEAR verdict.
runlog-selftest:
    scripts/runlog.sh selftest

# Prove the judge packet withholds the executor's account of itself, and
# that a verdict cannot attribute a gap without naming what it turns on.
judge-selftest:
    scripts/judge.sh selftest

# Scan the tracked tree for tenant-identifying data (block tier).
check-leaks:
    scripts/check-tenant-leaks.sh --all

# ─── Spec / Codegen ────────────────────────────────────

# Introspect the tenant GraphQL API and refresh spec/wiz-schema.graphql
# plus its sha256 pin. Needs service-account credentials.
sync-spec:
    cargo xtask sync-spec

# Validate every curated operation document against the vendored schema.
# A no-op (exit 0, loud warning) until sync-spec has landed the schema.
check-ops:
    cargo xtask check-ops

# Diff the vendored schema against the live one (not yet wired).
diff-spec:
    cargo xtask diff-spec

# ─── Setup ─────────────────────────────────────────────

setup:
    rustup component add clippy
    rustup toolchain install nightly --component rustfmt
    cargo install cargo-deny
    @echo "Optional: brew install lefthook && just install-hooks"

install-hooks:
    lefthook install

# ─── Maintenance ───────────────────────────────────────

clean:
    cargo clean
