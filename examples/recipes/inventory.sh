#!/usr/bin/env bash
# Recipe: fleet inventory — devices with blueprint context, as a table.
#
#   stave list blueprints > /tmp/blueprints.jsonl
#   stave list devices \
#     | stave enrich --with blueprint-context --blueprints /tmp/blueprints.jsonl \
#     | stave emit --format md
#
# Requires: stave auth login (token + subdomain) with read permissions.

set -euo pipefail

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

stave list blueprint > "$tmp"
stave list device \
  | stave enrich --with blueprint-context --blueprints "$tmp" \
  | stave emit --format md
