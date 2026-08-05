#!/usr/bin/env bash
# Recipe: vulnerability triage — critical/high first.
#
# list → CEL filter on severity → table. Add --param filters upstream
# (e.g. --param size=100) to widen or narrow the pull.

set -euo pipefail

stave list vulnerability \
  | stave filter --where 'severity in ["critical", "high"]' \
  | stave emit --format md
