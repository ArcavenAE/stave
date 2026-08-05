#!/usr/bin/env bash
# Recipe: resource inventory. Cloud resources with their owning
# account, as a table.
#
#   stave list cloud_account > accounts.jsonl
#   stave list cloud_resource \
#     | stave enrich --with account-context --accounts accounts.jsonl \
#     | stave emit --format md
#
# The account pull is a separate call because the join is client-side:
# cloud_resource.subscriptionExternalId matches cloud_account.externalId.
# Resources in a subscription no account claims come back with
# `account: null`, which is worth reading and not worth dropping.
#
# The md table renders a fixed four-column view (kind, id, severity,
# timestamp), so the joined account is in the stream but not in the
# table. Swap the last verb for `--format json` to read whole records
# pretty-printed, or pipe to jq for a column of your own:
#
#   ... | jq -r '[.name, (.account.name // "unowned")] | @tsv'
#
# Recorded as a v0.2 gap in README.md.
#
# Requires: stave auth login (client credentials with read scopes).

set -euo pipefail

accounts="$(mktemp)"
trap 'rm -f "$accounts"' EXIT

stave list cloud_account > "$accounts"
stave list cloud_resource \
  | stave enrich --with account-context --accounts "$accounts" \
  | stave emit --format md
