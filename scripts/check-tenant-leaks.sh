#!/usr/bin/env bash
# check-tenant-leaks.sh — block tenant-identifying data from entering git.
#
# stave operates against a live Wiz tenant. Payloads, audit lines,
# and even hostnames identify the cloud estate. This check runs generic
# patterns that are safe to publish, plus optional tenant-specific
# literals from a GITIGNORED local file — so the check itself never
# names the tenant.
#
# Usage:
#   scripts/check-tenant-leaks.sh --staged   # pre-commit (staged files)
#   scripts/check-tenant-leaks.sh --all      # CI / full-tree scan
#
# Local extension: create `.leak-patterns.local` (gitignored) with one
# fixed string per line — your tenant ID, region hostname, org name,
# cloud account IDs, internal hostnames. Lines starting with # are
# comments. Every developer working against a real tenant should
# create one.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
# Patterns live in one place so the detector and scripts/scrub.sh
# cannot drift apart. This script enforces the `block` tier only; the
# scrubber applies every tier. See scripts/leak-patterns.sh for why.
# shellcheck source=scripts/leak-patterns.sh
source scripts/leak-patterns.sh

MODE="${1:---staged}"

PLACEHOLDERS="$LEAK_PLACEHOLDERS"

# Blocking-tier patterns, PCRE, from the shared module.
GENERIC_PATTERNS=()
while IFS= read -r _p; do
  [[ -n "$_p" ]] && GENERIC_PATTERNS+=("$_p")
done < <(leak_blocking_patterns)

files() {
  case "$MODE" in
    --staged)
      git diff --cached --name-only --diff-filter=ACM
      ;;
    --all)
      git ls-files
      ;;
    *)
      echo "usage: $0 [--staged|--all]" >&2
      exit 2
      ;;
  esac
}

# Text files only; skip the vendored spec (vendor-published, uses
# placeholder servers) and this script itself.
#
# The rest of the exclusions share one criterion, and it is narrow: a
# file belongs here only if its JOB is to hold synthetic instances of
# the forbidden shapes. The hygiene scripts carry the patterns plus
# their own selftest values; `runlog.sh` plants a name, a resource name
# and an ARN and asserts none of them survive; `stub-stave` is the
# fixture it plants them from. A test that proves an ARN gets scrubbed
# has to contain something ARN-shaped, and rewriting the fixture to dodge
# this scan would hide the shape from the detector while still exercising
# it, which is worse than the exemption.
#
# The list is paths and not a self-declared marker on purpose. Adding one
# is a reviewable edit to a security control. A marker would let any new
# file exempt itself.
scan_list() {
  files | grep -vE '^(spec/|target/|scripts/(check-tenant-leaks|scrub|leak-patterns|runlog)\.sh$|examples/runlog/stub-stave$)' || true
}

fail=0
matches() {
  # $1 = pattern (PCRE), $2.. = files
  local pattern="$1"; shift
  [ "$#" -eq 0 ] && return 0
  PAT="$pattern" perl -ne '
    BEGIN { $re = qr/$ENV{PAT}/ }
    print "$ARGV:$.: $_" if /$re/;
    close ARGV if eof;
  ' -- "$@" 2>/dev/null \
    | grep -vE "https://api\.(${PLACEHOLDERS})\." \
    || true
}

FILE_LIST="$(scan_list)"
if [ -z "$FILE_LIST" ]; then
  exit 0
fi

# shellcheck disable=SC2086
for p in "${GENERIC_PATTERNS[@]}"; do
  hits="$(matches "$p" $FILE_LIST)"
  if [ -n "$hits" ]; then
    echo "tenant-leak check: pattern '$p' matched:" >&2
    echo "$hits" >&2
    fail=1
  fi
done

# Tenant-specific literals (gitignored local file — fixed strings).
if [ -f .leak-patterns.local ]; then
  while IFS= read -r needle; do
    case "$needle" in ''|'#'*) continue ;; esac
    # shellcheck disable=SC2086
    hits="$(grep -nF -- "$needle" $FILE_LIST 2>/dev/null || true)"
    if [ -n "$hits" ]; then
      echo "tenant-leak check: local pattern matched (value not echoed):" >&2
      echo "$hits" | cut -d: -f1,2 | sed 's/$/: <redacted match>/' >&2
      fail=1
    fi
  done < .leak-patterns.local
fi

if [ "$fail" -ne 0 ]; then
  cat >&2 <<'EOF'

Tenant-identifying data must not enter git. Sanitize before committing:
  - replace real region hostnames with api.<region>.app.wiz.io
  - replace tenant/account/resource ids with synthetic values
  - never commit audit-trail lines or raw API payloads
See SECURITY.md "Tenant Data Hygiene" and CONTRIBUTING.md.
EOF
  exit 1
fi
exit 0
