# Profiles and credential selection

Design document and survey. 2026-08-08. bd `aae-orc-ydto`, with
`aae-orc-58zw` (audit records the profile) and `aae-orc-aeq2` (the
safety coach is handed the resolved profile) still open.

---

## The problem

stave held one credential. We now have three in play: the read account,
the `stave-*-provisioner`, and the twelve-scope measurement account to
come. Nothing named them, nothing selected between them, and nothing
stopped the wrong one being used.

`STAVE_CLIENT_ID` plus `STAVE_CLIENT_SECRET` already override per
invocation through the B3 chains, so multiple credentials were
*possible*. They were unnamed, unaudited, invisible to the safety
coach, and carried the secret in the environment.

## Survey (2026-08-08)

| Tool | Name | Selection |
|---|---|---|
| AWS CLI | profile | `--profile`, `AWS_PROFILE`, `[profile x]` |
| gcloud | named configuration | `activate`, `--configuration`, `CLOUDSDK_ACTIVE_CONFIG_NAME` |
| spacectl | profile (alias) | `profile select` repoints `~/.spacelift/current` |
| gh | account | `gh auth switch`, `--hostname` |
| kubectl | context | `use-context`, `--context`, `KUBECONFIG` |
| Azure CLI | subscription | `az account set`, `--subscription` |
| Databricks | profile | `--profile`, `DATABRICKS_CONFIG_PROFILE` |
| Snowflake | connection | `--connection`, `set-default` |
| Doppler | directory-scoped config | chosen by cwd |

**Waxing.** gh shipped multi-account in v2.40.0, December 2023, the most
recent major CLI to add it. The name varies; the concept does not.

**No Wiz-native paradigm exists** to adopt instead. That is stave's
founding premise: there is no first-party general-purpose CLI. `wizcli`
is a scanner taking `--id`/`--secret` inline.

So we take the noun. "Profile" is what AWS, spacectl, and Databricks
call it, and it is the word Google's own docs reach for when explaining
named configurations.

## What the survey found that changed the design

Every surveyed tool ships **stateful global activation**, and every one
has the same footgun. GitHub documents it outright:

> `gh auth switch` applies machine-wide. It's not per-repo or
> per-directory. Other working directories are affected too. Get into
> the habit of running `gh auth status` to check the active account
> before starting work.

spacectl's `select` repoints a symlink. gcloud's `activate` is global.
In all three the active credential is invisible at the point of use, and
the shipped mitigation is *a habit the operator must remember*.

This repo keeps finding remembered controls insufficient; that is what
every behavior-trigger rule in `.claude/rules/` was written after. And
here the consequence is not a stale read. It is minting credentials in a
production security tenant under a profile nobody recalled was active.

**So: adopt the noun, reject the default.**

## The design

### Config

```toml
[default]
profile = "reader"          # may not name a provision-plane profile

[profile.reader]
client_id = "..."
purpose   = "day-to-day reads"
plane     = "read"          # read | provision, default read
enabled   = true            # absent means enabled

[profile.provisioner]
client_id = "..."
purpose   = "mints service accounts"
plane     = "provision"
```

The secret never appears here. It lives in the platform keyring under
`client-secret:<profile>`, namespaced so several accounts coexist. The
unnamed entry `client-secret` is untouched, so an install predating
profiles keeps working with no migration.

### Selection chain

`--profile` → `STAVE_PROFILE` → `[default] profile` → none, matching the
shape `cli-philosophy.md` prescribes, with an error naming every layer.

### Four refusals

1. **An unknown profile errors**, listing the known names. It never
   falls through to the unnamed credential, because falling through
   would run the command under a different identity than the one named.
2. **A disabled profile refuses even when named explicitly.** That is
   what `disable` is for.
3. **A provision-plane profile may not come from the stored default.**
   It is named per invocation or it is not used, so an unqualified
   command can never mint. This does not trip the abusive-argument test:
   `--profile` on every read would be abusive because near-constant, but
   naming a provisioner is rare and consequential, which the safety-flag
   exemption covers.
4. **A credential may only be used by its own plane.** The binary
   declares its plane at startup; an undeclared binary defaults to
   `read`, so forgetting to declare cannot reach a provisioning
   credential. No surveyed CLI attempts this, because no surveyed CLI
   has two planes with different blast radii.

A named profile also does **not** fall back to the unnamed keyring entry
or to `[auth] client_secret`. Absent means absent. Falling back is the
wrong-credential-used-by-accident failure in miniature.

### `profile list` never prints client IDs

They are credential identifiers, hygiene class 4. What an operator needs
to pick a profile is its purpose and plane. A list of opaque IDs helps
nobody choose and leaks something a list did not need to carry.

### `config set profile`, not `profile use`

Deliberate. gcloud's `activate` and spacectl's `select` read as a mode
switch; naming it what it is keeps the stored default visibly persistent
state. A provision-plane profile set there is refused at *resolution*,
not at write time, so the refusal names the call that would have used it
rather than the edit that configured it.

## Structure

`resolve_profile()` gathers the ambient inputs and calls
`select_profile(name, source, cfg, binary_plane)`, which is pure. The
split exists so the four refusals are unit-testable directly: this crate
forbids `unsafe`, so `std::env::set_var` is unavailable in edition 2024,
and a control worth having is worth testing without a subprocess.
Ambient wiring is covered by `crates/stave-cli/tests/profiles.rs` using
`assert_cmd`, which is how this repo already tests env-dependent
behaviour.

12 unit tests, 11 integration tests.

## Still open

- **`aae-orc-58zw`** — the audit line does not yet record which profile
  made the call. Until it does, the trail cannot distinguish a read
  under the narrow account from the same read under the 79-scope
  development credential, and the F4 mining surface inherits that
  ambiguity.
- **`aae-orc-aeq2`** — the safety coach still reviews command text only.
  With a stored default, the credential in play is absent from what it
  sees, which is `elem-control-scope-matches-reviewer-scope` applied to
  the coach.
- **Purpose enforcement beyond planes.** `purpose` is currently
  descriptive. Whether a finer contract (this profile may run these
  verbs) earns its weight is unsettled; the plane split covers the case
  that matters today.

## Cross-references

- `docs/design/credential-plane.md` — the two-plane split this serves
- `charter.md` B3 (the resolution chains), `.claude/rules/cli-philosophy.md`
- `crates/stave-sdk/src/auth.rs`, `crates/stave-cli/tests/profiles.rs`
