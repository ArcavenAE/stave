#!/usr/bin/env bash
# scrub.sh: make stave output safe to read, paste, and quote.
#
# The detector (check-tenant-leaks.sh) stops tenant data at git entry.
# It cannot help with the earlier and more common leak: output read
# into a terminal, a chat transcript, an issue body, or an agent's
# context. Once that has happened there is nothing to block. This
# script is the transform that runs BEFORE the read.
#
# Usage:
#   stave list issue --limit 50 | scripts/scrub.sh
#   scripts/scrub.sh --catalog < findings.jsonl
#   scripts/scrub.sh --selftest        # synthetic; no tenant needed
#
# Two layers, both applied:
#
#   1. FIELD layer (stave JSONL only): allowlist, DEFAULT-DENY. Only
#      fields known to be non-identifying survive; everything else
#      becomes "<redacted:fieldname>". Default-deny is the whole point:
#      when the vendor adds a field, or a curated document grows a
#      selection, the new field is redacted until someone classifies
#      it. A denylist would have leaked it on first sight.
#
#   2. TEXT layer (everything): structural shapes and local literals
#      from scripts/leak-patterns.sh. Catches what survives layer 1,
#      what appears in error messages and audit lines, and anything
#      that is not JSON at all.
#
# The field layer is what regex cannot do. A person's name in
# `entitySnapshot.name` has no shape to match; it is caught because
# that field is not on the allowlist, not because it looked dangerous.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
# shellcheck source=scripts/leak-patterns.sh
source scripts/leak-patterns.sh

MODE=auto
CATALOG=0
SELFTEST=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --jsonl) MODE=jsonl ;;
    --text) MODE=text ;;
    --auto) MODE=auto ;;
    --catalog) CATALOG=1 ;;
    --selftest) SELFTEST=1 ;;
    -h|--help)
      sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "scrub.sh: unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

# ---------------------------------------------------------------------
# Field policy
# ---------------------------------------------------------------------
#
# SAFE: enums, booleans, counts, timestamps, and stave's own stream
# metadata. None of these narrow to a tenant, an account, a resource,
# or a person. Severity and status describe posture in the abstract;
# it is the entity they attach to that identifies, and that entity is
# redacted.
# `entitySnapshot` is allowed as a CONTAINER, not as a value: the walk
# recurses into it and its children face the same allowlist, so `type`
# and `cloudPlatform` survive while `name`, `id`, and
# `subscriptionExternalId` are redacted individually. Allowing the
# container is what makes "open criticals by cloud platform" possible
# without naming a single resource.
SAFE_FIELDS='[
  "_kind","_source","operation_id","response_index","fetched_at",
  "schema_version","result","entitySnapshot",
  "severity","severity_rollup","vendorSeverity","status","type",
  "entity_type","action","enabled","archived","provisional",
  "cloudProvider","cloudPlatform","entity_cloud_platform",
  "resourceCount",
  "createdAt","updatedAt","resolvedAt","dueAt","timestamp",
  "firstDetectedAt","lastDetectedAt","lastScannedAt",
  "op_type","root_field","sensitivity","cost_hint",
  "required_scopes","scopes_provisional","effects"
]'

# OWN_REGISTRY kinds carry no tenant data at all; they are stave's own
# curated operation metadata, identical on every machine. Passed
# through the field layer untouched (the text layer still runs).
OWN_KINDS='["operation","operation_permissions"]'

# CATALOG kinds MAY have vendor-published names (control titles, CVE
# ids, framework names). Those are safe to show. But a tenant can also
# author CUSTOM controls and CUSTOM frameworks named after the org, and
# nothing in the record distinguishes the two. So names stay redacted
# by default and --catalog is the operator saying "I looked."
CATALOG_KINDS='["control","cloud_config_rule","security_framework","vulnerability_finding"]'

field_layer() {
  jq -c \
    --argjson safe "$SAFE_FIELDS" \
    --argjson own "$OWN_KINDS" \
    --argjson catkinds "$CATALOG_KINDS" \
    --argjson catalog "$CATALOG" '
    def scrub($safe; $extra):
      if type == "object" then
        . as $in
        | reduce ($in | keys_unsorted[]) as $k ({};
            . + { ($k):
                  ( if ($safe | index($k)) or ($extra | index($k))
                    then ($in[$k] | scrub($safe; $extra))
                    else "<redacted:\($k)>"
                    end ) })
      elif type == "array" then map(scrub($safe; $extra))
      else . end;
    . as $r
    | (($r | type) == "object") as $isobj
    | if $isobj | not then $r
      else
        ($r._kind // "") as $kind
        | if ($own | index($kind)) then $r
          else
            (if ($catalog == 1) and ($catkinds | index($kind))
             then ["name","description"] else [] end) as $extra
            | $r | scrub($safe; $extra)
          end
      end
  '
}

text_layer() {
  local rules="$LEAK_STRUCTURAL_RULES"
  local localfile
  localfile="$(leak_local_file)"
  if [[ -n "$localfile" ]]; then
    # Local literals are fixed strings; quotemeta them into rules.
    while IFS= read -r needle; do
      case "$needle" in ''|'#'*) continue ;; esac
      rules+=$'\n'"\\Q${needle}\\E"$'\t'"<REDACTED>"
    done < "$localfile"
  fi
  SCRUB_RULES="$rules" perl -e '
    my @rules;
    for my $line (split /\n/, ($ENV{SCRUB_RULES} // "")) {
      next unless length $line;
      # Rules carry "<pattern>\t<replacement>\t<tier>"; the scrubber
      # applies every tier, so the tier column is read and ignored.
      # Local literals appended by the caller have no tier column.
      my ($p, $r, $tier) = split /\t/, $line, 3;
      next unless defined $r;
      push @rules, [qr/$p/, $r];
    }
    while (my $l = <STDIN>) {
      for my $rule (@rules) {
        my ($re, $rep) = @$rule;
        $l =~ s/$re/$rep/g;
      }
      print $l;
    }
  '
}

looks_like_stave_jsonl() {
  # First non-blank line parses as a JSON object carrying `_kind`.
  local first
  first="$(grep -m1 . "$1" || true)"
  [[ -n "$first" ]] && printf '%s' "$first" | jq -e 'type == "object" and has("_kind")' >/dev/null 2>&1
}

run_scrub() {
  local tmp
  tmp="$(mktemp)"
  trap 'rm -f "$tmp"' RETURN
  cat > "$tmp"
  local use_field=0
  case "$MODE" in
    jsonl) use_field=1 ;;
    text) use_field=0 ;;
    auto) looks_like_stave_jsonl "$tmp" && use_field=1 ;;
  esac
  if [[ "$use_field" -eq 1 ]]; then
    field_layer < "$tmp" | text_layer
  else
    text_layer < "$tmp"
  fi
}

# ---------------------------------------------------------------------
# Selftest: synthetic values only, safe to commit and to run in CI.
# ---------------------------------------------------------------------
selftest() {
  local fail=0
  check() { # name, input, must-not-contain
    local name="$1" input="$2" forbidden="$3" out
    out="$(printf '%s\n' "$input" | run_scrub)"
    if printf '%s' "$out" | grep -qF -- "$forbidden"; then
      printf 'FAIL %-28s leaked: %s\n' "$name" "$forbidden" >&2
      printf '     output: %s\n' "$out" >&2
      fail=1
    else
      printf 'ok   %-28s\n' "$name"
    fi
  }

  check "field: entity name (PII)" \
    '{"_kind":"issue","severity":"HIGH","entitySnapshot":{"type":"USER_ACCOUNT","name":"Jane Q Example"}}' \
    'Jane Q Example'
  check "field: entity id inside allowed container" \
    '{"_kind":"issue","entitySnapshot":{"type":"VM","cloudPlatform":"AWS","id":"deadbeef-cafe","name":"prod-host"}}' \
    'prod-host'
  check "field: resource name" \
    '{"_kind":"cloud_resource","type":"VM","name":"prod-db-primary","nativeType":"i"}' \
    'prod-db-primary'
  check "field: subscription name" \
    '{"_kind":"cloud_resource","type":"VM","subscriptionName":"Contoso Production"}' \
    'Contoso Production'
  check "field: project slug" \
    '{"_kind":"project","archived":false,"slug":"acme-internal"}' \
    'acme-internal'
  check "field: record id" \
    '{"_kind":"issue","severity":"LOW","id":"abcd1234"}' \
    'abcd1234'
  check "field: unknown new field" \
    '{"_kind":"issue","severity":"LOW","brandNewUpstreamField":"secret-value"}' \
    'secret-value'
  check "text: email" \
    'contact svc@example-corp.com now' \
    'svc@example-corp.com'
  check "text: OCID" \
    'ocid1.compartment.oc1..aaaaaaaaexamplecompartmentid00000' \
    'ocid1.compartment.oc1..aaaaaaaaexamplecompartmentid00000'
  check "text: ARN" \
    'arn:aws:iam::123456789012:role/ExampleRole' \
    'arn:aws:iam::123456789012:role/ExampleRole'
  check "text: GCP resource path" \
    'projects/example-project-dev/serviceAccounts/x' \
    'projects/example-project-dev/serviceAccounts/x'
  check "text: azure subscription path" \
    '/subscriptions/00000000-1111-2222-3333-444444444444/resourceGroups/rg1' \
    '/subscriptions/00000000-1111-2222-3333-444444444444/resourceGroups/rg1'
  check "text: bare account id" \
    'account 123456789012 scanned' \
    '123456789012'
  check "text: GUID" \
    'id 00000000-1111-2222-3333-444444444444 seen' \
    '00000000-1111-2222-3333-444444444444'
  check "text: bearer token" \
    'Authorization: Bearer abcdefghij0123456789xyz' \
    'abcdefghij0123456789xyz'
  check "text: region hostname" \
    'POST https://api.us999.app.wiz.io/graphql' \
    'api.us999.app.wiz.io'
  check "text: IP address" \
    'peer 203.0.113.42 refused' \
    '203.0.113.42'

  # Positive control: safe fields must SURVIVE, or the scrubber is
  # useless rather than merely safe.
  local kept
  kept="$(printf '%s\n' '{"_kind":"issue","severity":"CRITICAL","status":"OPEN"}' | run_scrub)"
  if printf '%s' "$kept" | grep -q 'CRITICAL' && printf '%s' "$kept" | grep -q 'OPEN'; then
    printf 'ok   %-28s\n' "positive control: kept"
  else
    printf 'FAIL %-28s safe fields were destroyed: %s\n' "positive control" "$kept" >&2
    fail=1
  fi

  if [[ "$fail" -ne 0 ]]; then
    echo >&2
    echo "scrub.sh selftest FAILED" >&2
    exit 1
  fi
  echo
  echo "scrub.sh selftest passed"
}

if [[ "$SELFTEST" -eq 1 ]]; then
  selftest
else
  run_scrub
fi
