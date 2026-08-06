# Implementation Plan: Read-Only Posture and the Permissions Report

Implements the decisions in
`docs/design/read-only-posture-and-permissions-report.md` (D1 through D11).
Drafted 2026-08-05. This plan and every test description in it are governed by
the safety rules below.

## MANDATORY SAFETY RULES FOR IMPLEMENTATION AND TESTING

The only available Wiz tenant is IN-USE PRODUCTION. These rules bind all
implementation work, all tests, all CI, and all manual verification of this
feature, now and in the future.

1. **Read-only, passive activities ONLY.** No step of implementing or testing
   this feature may perform any ACTIVE, CHANGE, REMEDIATION, or DESTRUCTIVE
   action against the Wiz tenant. This ban includes, without limitation:
   - changing any tenant configuration or setting;
   - initiating, scheduling, or triggering any scan;
   - creating, modifying, or deleting any tenant object (issues, reports,
     rules, users, service accounts, projects, integrations, anything);
   - causing the tenant to run, scan, generate, notify, or otherwise take
     any action (report generation and integration triggers count as
     actions);
   - any remediation operation.
2. **The ban applies even to testing the guard itself.** We deliberately err
   on the side of "the delete block is NOT TESTED" rather than validating it
   against production. A mutation attempted "to confirm the refusal works"
   is a violation, full stop.
3. **Automated tests never contact the tenant.** Every test in this feature
   is hermetic: unit tests, local wiremock mock servers, synthetic JWTs, and
   sandboxed config/state/audit directories. No test may open a connection
   to `api.<region>.app.wiz.io`, `auth.app.wiz.io`, `mcp.app.wiz.io`, or any
   other Wiz endpoint. The existing sandbox harness (env-stripped, keyring
   disabled, temp dirs) is the required baseline for every new test.
4. **Guard verification is done by absence, not by action.** To verify a
   refusal, tests assert the client refuses BEFORE the wire, using a local
   mock server configured to expect ZERO requests. A request arriving at the
   mock is a test failure. No verification path may consist of "send the
   mutation and see if the server rejects it."
5. **These rules must be restated** in the module docs of every new test
   file this feature adds, and in any future test description, PR body, or
   follow-on plan that touches this feature.

## Phases

### Phase A: D10, remove `allow_writes` from the typed model

Remove flag, env var, config key, status reporting, and SDK opt-in:

- `stave-sdk/src/client.rs`: delete `CallOptions.allow_write`; the mutation
  check in `call_op` and `call_document` refuses unconditionally.
- `stave-sdk/src/auth.rs`: delete `ALLOW_WRITE_ENV`, `resolve_allow_writes`,
  and `Config.default.allow_writes` (the struct is `#[serde(default)]`, so a
  stale key still parses; emit a one-line "obsolete and ignored" warning via
  `tracing::warn!` when the raw config contains it).
- `stave-cli/src/main.rs`: delete `--allow-write` flags (`api`, `mcp call`),
  `resolve_allow_write`, the `allow_writes` config set/unset key, and the
  `writes` line in `auth status`.
- MCP `tools/call` for non-read-shaped tools refuses unconditionally (same
  wall as GraphQL mutations).

### Phase B: D2 refusal text and rule amendment

- `stave-sdk/src/error.rs`: `WriteGuard` display becomes the terminal,
  byte-stable wall text. It names no flag, env var, or config key.
- `.claude/rules/cli-philosophy.md`: add the maps-vs-walls split (chain
  errors get repair maps; guard refusals get terminal walls).
- `SECURITY.md`: the D8 statement (guard exists, never commissioned against
  a live mutation, lean on scopes).

### Phase C: D3 + D4 + D4a registry metadata and gate

- `stave-api/src/lib.rs`: `OperationDoc` gains `required_scopes`
  (`&'static [&'static str]`), `effects` (reversibility, side effects,
  egress), `sensitivity`, `cost_hint`. All twelve read operations annotated;
  scope names carry a `provisional` marker at the registry level until F1.
- `xtask` `check-ops`: fail when any operation has empty `required_scopes`.
- Unit test in `stave-api` asserting the same invariant.

### Phase D: D5 verbs

- SDK `token.rs`: tolerant scope-claim decoder (tries `scope`
  space-delimited string, then `scp`, then `permissions` array; reports
  which field matched). Provisional until F1.
- CLI:
  - `stave ops permissions`: static registry report (offline).
  - `stave auth scopes`: decoded claim of the token at hand (env or cache;
    never mints).
  - `stave auth can-i <op>`: required subset-of granted; exit 0/1;
    `read:all` treated as granting `read:*` (provisional rule).
  - `stave auth plan [--ops ...] [--check]`: GRANT and DO NOT GRANT
    sections, two-account doctrine, `read:all` discouraged; `--check` exits
    nonzero on drift and separately reports MISSING (unusable) vs EXCESS
    (over-privileged).
- All scope-dependent output carries `"provisional": true` until F1.

### Phase E: D6 audit changes

- `schema_version` 2 to 3 (new `result` value and new fields are a contract
  change per the audit doc).
- New `result: "refused"`: guard refusals construct a minimal span and emit
  it before returning the error (control-flow change in `call_op` /
  `call_document` / MCP call path).
- Session identity contract: `STAVE_SESSION_ID` env var, opaque string
  supplied by the invoking environment, recorded in the invocation block
  when present. The per-session refusal detector consumes it downstream.
- `docs/audit-trail-format.md` updated (version, `refused`, `session_id`,
  `posture`, `document_sha256`).

### Phase F: D11 exploratory-read posture

- Config `[default] posture = "curated" | "exploratory"`, default curated.
- `stave api --query <doc>` (ad-hoc documents) refuses under curated
  posture. Mutation/subscription refusal stays unconditional in BOTH
  postures and is checked FIRST. No per-call override exists.
- Audit records the active posture and the ad-hoc document's sha256.
- `stave config set posture` accepts only the two named values.

### Phase G: docs and charter

- Charter: F6 summary + pointer to the design brief (light touch).
- README: note the read-only posture and the permission verbs.

## Safe test inventory

Every test below is hermetic per the MANDATORY SAFETY RULES: sandboxed env,
local wiremock or no network at all, synthetic JWTs, zero tenant contact.
None of these tests performs any active, change, remediation, or destructive
action against any real system; the production tenant is never contacted.

Phase A/B (guard):
- `mutation_refuses_with_no_override_available`: curated mutation document
  via `api --query` against a wiremock server that expects ZERO requests;
  assert refusal text is the wall, assert the mock received nothing.
- `allow_write_env_is_ignored`: same, with `STAVE_ALLOW_WRITE=1` set;
  refusal unchanged (the env var no longer exists).
- `stale_allow_writes_config_key_is_ignored_and_warned`: config containing
  `allow_writes = true`; mutation still refused; warning mentions obsolete.
- `refusal_text_is_byte_stable`: two invocations, identical stderr message.
- `refusal_names_no_override`: assert the message contains no flag, env, or
  config token.
- `mcp_non_read_tool_refuses_unconditionally`: local mock MCP; zero calls.

Phase C:
- `every_operation_declares_required_scopes` (stave-api unit).
- `check_ops_fails_on_missing_scopes` (xtask, against a synthetic registry
  fixture).

Phase D (all offline or fake-JWT):
- `ops_permissions_lists_scopes_and_tiers_offline`: no server at all.
- `auth_scopes_decodes_scope_claim_variants`: synthetic JWTs with `scope`
  string / `scp` array / `permissions` array / no claim.
- `can_i_yes_no_and_exit_codes`: synthetic JWT vs registry.
- `can_i_read_all_wildcard_is_provisional`.
- `auth_plan_grant_and_do_not_grant_sections`.
- `auth_plan_check_distinguishes_missing_from_excess`: three fixtures
  (missing only, excess only, both); exit codes and section content.
- `scope_output_carries_provisional_marker`.

Phase E:
- `refused_call_emits_audit_line_with_result_refused`: guard trip writes a
  JSONL line; wiremock expects zero requests.
- `audit_lines_carry_schema_version_3`.
- `session_id_env_is_recorded_when_present` / `absent_when_unset`.

Phase F:
- `adhoc_read_refused_under_curated_posture`: zero-request mock.
- `adhoc_read_allowed_under_exploratory_posture`: local wiremock (a mock
  server on 127.0.0.1 is not the tenant).
- `mutation_refused_under_exploratory_posture_too`: zero-request mock.
- `audit_records_posture_and_document_hash`.
- `config_set_posture_rejects_unknown_values`.

Explicitly BANNED tests (do not write these, ever): any test issuing a
mutation to a real endpoint; any test that logs in to the real tenant and
"just lists" as part of CI; any test creating a report, starting a scan, or
changing tenant/service-account configuration; any live verification that
the write/delete block "really works." The block ships UNTESTED against the
live tenant by design.

## Sequencing and verification

A → B → C → D → E → F → G, full quality gate after each phase
(`cargo +nightly fmt`, `clippy -D warnings`, `cargo test --workspace`,
`cargo deny`, leak scan). Live validation (F1) later confirms scope names
and claim shape and lifts the provisional markers; F1 is itself read-only
passive (list queries only) per the safety rules above.
