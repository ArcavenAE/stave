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

MODE="${1:---staged}"

# Placeholder region/tenant tokens that are allowed to appear with real
# host suffixes (docs, vendor examples, tests).
PLACEHOLDERS='<region>|region|your-region|example'

# Generic patterns (ERE). Safe to publish — they describe *shapes*,
# not values.
GENERIC_PATTERNS=(
  # UUID-shaped bearer tokens in pasted commands/output
  'Bearer [0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}'
  # Region-bearing Wiz API hostnames that are not known placeholders
  # (the region narrows the tenant; docs use api.<region>.app.wiz.io)
  'https://api\.[a-z]{2,4}[0-9]{1,3}\.app\.wiz\.io'
  # Registry usernames embed the tenant ID
  'wizio-repo-[0-9a-f-]{8,}'
)

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
scan_list() {
  files | grep -vE '^(spec/|target/|scripts/check-tenant-leaks\.sh$)' || true
}

fail=0
matches() {
  # $1 = pattern (ERE), $2.. = files
  local pattern="$1"; shift
  [ "$#" -eq 0 ] && return 0
  grep -nE "$pattern" -- "$@" 2>/dev/null \
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
