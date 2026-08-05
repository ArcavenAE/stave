#!/usr/bin/env bash
# Recipe: issue triage. The critical and high issues, as a table.
#
#   stave list issue \
#     | stave enrich --with entity-hoist \
#     | stave filter --where 'severity in ["CRITICAL", "HIGH"]' \
#     | stave emit --format md
#
# entity-hoist runs first so the affected resource is reachable as a
# flat field (entity_name, entity_type, entity_cloud_platform) rather
# than a nested entitySnapshot path. Predicates downstream of it can
# read the resource, and it rides the stream to whatever consumes the
# JSONL.
#
# Note that `emit --format md` renders a fixed four-column view (kind,
# id, severity, timestamp), so the hoisted fields are in the stream but
# not in the table. Swap the last verb for `emit --format json` to read
# them: it pretty-prints one array of whole records, which is easier on
# the eye than raw jsonl. Recorded as a v0.2 gap in README.md.
#
# Requires: stave auth login (client credentials with read scopes).

set -euo pipefail

stave list issue \
  | stave enrich --with entity-hoist \
  | stave filter --where 'severity in ["CRITICAL", "HIGH"]' \
  | stave emit --format md
