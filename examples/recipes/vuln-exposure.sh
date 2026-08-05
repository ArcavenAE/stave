#!/usr/bin/env bash
# Recipe: vulnerability exposure. Open findings, severity normalized,
# newest exposure first.
#
#   stave list vulnerability_finding \
#     | stave enrich --with severity-roll-up \
#     | stave filter --where 'status == "OPEN"' \
#     | stave emit --format md
#
# severity-roll-up copies vendorSeverity into severity_rollup, so one
# predicate name serves a stream drawn from findings (vendorSeverity) or
# from issues (severity) without knowing which it got. The md table
# reads the kind's own severity field, so it stays correct either way.
#
# Requires: stave auth login (client credentials with read scopes).

set -euo pipefail

stave list vulnerability_finding \
  | stave enrich --with severity-roll-up \
  | stave filter --where 'status == "OPEN"' \
  | stave emit --format md
