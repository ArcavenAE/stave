#!/usr/bin/env bash
# leak-patterns.sh: the single source of truth for tenant-leak patterns.
#
# Sourced by both halves of the hygiene tooling so they cannot drift:
#   * scripts/check-tenant-leaks.sh  DETECT (blocks git entry)
#   * scripts/scrub.sh               TRANSFORM (makes output safe to read)
#
# A pattern added here is enforced by the detector and neutralised by
# the scrubber in the same commit. That coupling is the point: a
# detector that knows a shape the scrubber cannot fix just blocks work,
# and a scrubber the detector does not know about rots silently.
#
# Two tiers:
#   STRUCTURAL: shapes, safe to publish. Live in this file.
#   LOCAL:      literal values (tenant id, region host, org name).
#                Live in a GITIGNORED `.leak-patterns.local`, one fixed
#                string per line, never in git.
#
# Regex dialect is PCRE (perl), not ERE. Both consumers run patterns
# through perl so there is exactly one dialect to reason about and no
# BSD/GNU grep divergence between a developer's pre-commit and CI.

# Placeholder tokens allowed in host position (docs, tests, vendor
# examples). PCRE alternation.
LEAK_PLACEHOLDERS='<region>|region|your-region|example'

# STRUCTURAL_RULES: one rule per line, "<PCRE>\t<replacement>\t<tier>".
#
# TIERS. The two consumers want different thresholds, and conflating
# them is how this kind of tooling dies:
#
#   block  Both scrubbed and blocked at git entry. Reserved for shapes
#          with no benign occurrence in this repo. Measured 2026-08-06
#          across all tracked files: zero hits each.
#   scrub  Scrubbed only, never blocks a commit. Shapes that are real
#          leaks in live output but appear legitimately in synthetic
#          fixtures, tests, and docs. Measured: GUID 7 files, 12-digit
#          12 files, IP 5 files, email 1, broad-Bearer 1, every one of
#          them benign.
#
# Over-redacting output costs nothing. Over-blocking commits stops
# work, and a checker that cries wolf gets disabled within a week.
# Promote scrub to block only after measuring zero benign hits.
#
# Order matters for the scrubber: specific shapes first, so a broad
# rule cannot eat a narrow one's match. GUID sits below the resource
# shapes that embed one; the bare-number account id sits last.
read -r -d '' LEAK_STRUCTURAL_RULES <<'RULES' || true
ocid1\.[a-z0-9]+\.[a-z0-9-]+\.[a-z0-9-]*\.[a-z0-9]{20,}	<OCID>	block
arn:aws[a-z-]*:[a-z0-9-]*:[a-z0-9-]*:[0-9]{12}:[^\s"',]+	<ARN>	block
projects/[a-z][a-z0-9-]{4,28}[a-z0-9]/[A-Za-z]+/[^\s"',]+	<GCP_RESOURCE>	block
/subscriptions/[0-9a-fA-F-]{36}/[^\s"',]+	<AZURE_RESOURCE>	block
eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}	<JWT>	block
(?i)bearer\s+[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}	Bearer <TOKEN>	block
https://api\.[a-z]{2,4}[0-9]{1,3}\.app\.wiz\.io	https://api.<region>.app.wiz.io	block
wizio-repo-[0-9a-fA-F-]{8,}	<REGISTRY_USER>	block
(?i)bearer\s+[A-Za-z0-9._-]{16,}	Bearer <TOKEN>	scrub
[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}	<EMAIL>	scrub
[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}	<GUID>	scrub
\b\d{12}\b	<ACCOUNT_ID>	scrub
\b(?:\d{1,3}\.){3}\d{1,3}\b	<IP>	scrub
RULES

# Pattern column for the blocking tier only (the detector).
leak_blocking_patterns() {
  printf '%s\n' "$LEAK_STRUCTURAL_RULES" | while IFS=$'\t' read -r pat _repl tier; do
    [[ -n "$pat" && "$tier" == "block" ]] && printf '%s\n' "$pat"
  done
  return 0
}

# Pattern column for every tier (diagnostics, the leak-scan skill).
leak_structural_patterns() {
  printf '%s\n' "$LEAK_STRUCTURAL_RULES" | while IFS=$'\t' read -r pat _repl _tier; do
    [[ -n "$pat" ]] && printf '%s\n' "$pat"
  done
  return 0
}

# Path to the gitignored local literals file, if the repo has one.
leak_local_file() {
  # Resolved from THIS file's location, never the caller's CWD. A harness
  # driven from the orchestrator root or a scratch directory would
  # otherwise silently pick up the wrong literals, or none.
  local here root
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  root="$(dirname "$here")"
  [[ -f "$root/.leak-patterns.local" ]] && printf '%s\n' "$root/.leak-patterns.local"
  return 0
}
