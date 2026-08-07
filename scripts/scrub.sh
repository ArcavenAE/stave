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
# Resolve the pattern module from this script's own location so the
# scrubber works when invoked from any directory. Scrub-by-construction
# in a run harness means being called from somewhere else.
SCRUB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/leak-patterns.sh
source "$SCRUB_DIR/leak-patterns.sh"

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
#
# The `cloud_resource_v2` additions below are the same three classes
# already stated above (booleans, counts, enums) applied to a wider
# selection. What stays redacted there is what identifies: `name`,
# `region`, `externalId`, `providerUniqueId`, `tags` (tenant-authored),
# `projects` (org-named), `cloudAccount`, and the GraphEntity fields
# behind `owners`, `codeRepository`, `iacDeployment`, `iacModuleSource`.
# `owners` and the two analytics objects are allowed as CONTAINERS on
# the same reasoning as `entitySnapshot`: the walk recurses and the
# children face the allowlist, so an owner's `type` (the attribution
# basis) survives while the owner's identity does not.
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
  "required_scopes","scopes_provisional","effects",
  "isAccessibleFromInternet","isOpenToAllInternet",
  "hasSensitiveData","hasAccessToSensitiveData",
  "firstSeen","lastSeen","deletedAt",
  "owners",
  "iacDetails","iacStatus","iacPlatform",
  "iacDetectionMethod","iacDriftDetectionMethod",
  "issueAnalytics","vulnerabilityAnalytics",
  "issueCount","criticalSeverityCount","highSeverityCount",
  "mediumSeverityCount","lowSeverityCount","informationalSeverityCount",
  "totalFindingCount","criticalSeverityFindingCount",
  "highSeverityFindingCount","mediumSeverityFindingCount",
  "lowSeverityFindingCount","informationalSeverityFindingCount",
  "lastLoginAt","lastRotatedAt","expiresAt","firstScannedAt",
  "statusChangedAt","reopenedAt","rejectionExpiredAt",
  "cisaKevDueDate","lastRunAt","lastSuccessfulRunAt","lastSeenAt",
  "isSuspended","hasFix","hasExploit",
  "actionType","openReason","resolutionReason","fixedVersion",
  "criticalSystemHealthIssueCount","highSystemHealthIssueCount"
]'

# The 2026-08-07 widening (bd aae-orc-qijl) made nine kinds ask for more
# fields, and default-deny redacted nearly all of them. That left runbook
# A4 reachable at the API and unreadable at the terminal in the same
# change: `createdAt` survived while `lastLoginAt` and `lastRotatedAt`,
# which are the runbook's entire question, did not. Those are the same
# class of data as the timestamps already allowed above, so the
# inconsistency was in the allowlist and not in the request.
#
# What was added is timestamps, booleans, enums, and counts. Nothing
# else.
#
# What was deliberately NOT added, because each identifies something:
#
#   sourceIP           an IP address; tenant-data-hygiene names these
#   performer          who did it
#   assignee           who owns it
#   resolvedBy         who closed it
#   actionParameters   arbitrary JSON from an audit entry
#   projects           org-named
#   linkedProjects     org-named
#   projectOwners      people
#   securityChampions  people
#   businessUnit       org structure
#   tags               tenant-authored, the usual home of owner emails
#   serviceTickets     ticket ids and URLs
#   vulnerableAsset    the resource itself
#   rootComponent      package and path
#   layerMetadata      image layer identity
#   sourceRules        may carry tenant-authored rule names
#   effectiveRole      may carry tenant-authored custom role names
#
# `scopes` is the interesting refusal and is deliberate. Scope names are
# not tenant-identifying on their own, and bd aae-orc-8af5 wants the
# field. But a SCRUBBED service-account inventory showing which accounts
# hold admin grants is precisely the targeting map this rule exists to
# prevent. 8af5 reads it raw in a terminal under the hygiene rule's
# narrow exception, and it never reaches a durable artifact.

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
                    # A null is not tenant data. Rendering it as
                    # "<redacted:field>" made absent and populated
                    # byte-identical, which erased the only question the
                    # live-validation queue asks: is this field
                    # populated. Discloses nothing; the value is still
                    # gone in every non-null case.
                    elif $in[$k] == null then null
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

# Classify the input so dispatch can FAIL CLOSED.
#
# The original version asked one question, "is the first line a JSON
# object with _kind", and silently fell through to the text tier for
# everything else. Measured 2026-08-06: that meant the field layer
# engaged on exactly one of the four shapes stave actually emits, and a
# person's name passed through `emit --format json` and
# `emit --format md` untouched, exit 0, no warning. Silent fall-through
# on the shape that carries the most identifying data is the worst
# possible default for a leak control.
classify_input() {
  local f="$1" first
  first="$(grep -m1 . "$f" || true)"
  if [[ -z "$first" ]]; then
    printf 'empty\n'; return
  fi
  # A rendered markdown table: pipes and a separator row.
  if printf '%s' "$first" | grep -qE '^\|.*\|$'; then
    printf 'md-table\n'; return
  fi
  # Pretty-printed JSON array (emit --format json).
  if printf '%s' "$first" | grep -qE '^[[:space:]]*\[[[:space:]]*$'; then
    if jq -e 'type == "array" and (length == 0 or (.[0] | type == "object" and has("_kind")))' \
        < "$f" >/dev/null 2>&1; then
      printf 'json-array\n'; return
    fi
    printf 'json-unknown\n'; return
  fi
  # Newline-delimited stave records.
  if printf '%s' "$first" | jq -e 'type == "object" and has("_kind")' >/dev/null 2>&1; then
    printf 'jsonl\n'; return
  fi
  # A JSON document with no _kind: `stave api` raw GraphQL output.
  if printf '%s' "$first" | grep -qE '^[[:space:]]*\{' ; then
    printf 'json-unknown\n'; return
  fi
  printf 'text\n'
}

refuse() {
  cat >&2 <<EOF
scrub.sh: refusing to process $1.

$2

Refusing rather than falling through to the pattern tier, because the
pattern tier cannot see the class that matters. A resource name, a
bucket, a project slug, or a person's name has no shape to match; those
are caught by the field allowlist or not at all.
EOF
  exit 3
}

run_scrub() {
  local tmp shape
  tmp="$(mktemp)"
  trap 'rm -f "$tmp"' RETURN
  cat > "$tmp"

  # An explicit --text is the operator accepting the pattern tier alone.
  if [[ "$MODE" == "text" ]]; then
    text_layer < "$tmp"
    return
  fi

  shape="$(classify_input "$tmp")"

  if [[ "$MODE" == "jsonl" && "$shape" != "jsonl" && "$shape" != "json-array" ]]; then
    refuse "input that is not a stave record stream (--jsonl was given, shape looks like: $shape)" \
      "Re-run without --jsonl to see the shape-specific guidance."
  fi

  case "$shape" in
    empty)
      : ;;
    jsonl)
      field_layer < "$tmp" | text_layer ;;
    json-array)
      # emit --format json. Explode to records, field-scrub, rebuild.
      jq -c '.[]' < "$tmp" | field_layer | jq -s '.' | text_layer ;;
    md-table)
      refuse "a rendered markdown table" \
"A table has already lost its field names, so the allowlist cannot be
applied to it. The id column is a raw record id.

Scrub BEFORE emit, not after:
  stave list issue --limit 50 | scripts/scrub.sh | stave emit --format md" ;;
    json-unknown)
      refuse "a JSON document with no \`_kind\`" \
"This is the shape \`stave api --query\` produces. The field allowlist is
keyed to the twelve curated kinds and cannot classify an arbitrary
GraphQL response, so it would pass every field through.

Either project the response into the record stream first, or accept the
pattern tier alone with an explicit --text and read the result knowing
names and slugs survive it." ;;
    text)
      refuse "input of unrecognised shape" \
"If this is prose, an error message, or a log line, pass --text to accept
the pattern tier alone. That tier catches emails, GUIDs, ARNs, OCIDs,
IPs, account ids, and the local literals. It does NOT catch names,
bucket names, or project slugs." ;;
  esac
}

# ---------------------------------------------------------------------
# Selftest: synthetic values only, safe to commit and to run in CI.
# ---------------------------------------------------------------------
selftest() {
  local fail=0
  check() { # name, input, must-not-contain [, mode]
    local name="$1" input="$2" forbidden="$3" mode="${4:-auto}" out
    local saved="$MODE"; MODE="$mode"
    out="$(printf '%s\n' "$input" | run_scrub)"
    MODE="$saved"
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
  check "field: v2 tag value" \
    '{"_kind":"cloud_resource_v2","isOpenToAllInternet":true,"tags":[{"key":"owner","value":"payments-team"}]}' \
    'payments-team'
  check "field: v2 owner identity" \
    '{"_kind":"cloud_resource_v2","owners":[{"type":"DECLARED_OWNER","graphEntity":{"name":"Jane Q Example"}}]}' \
    'Jane Q Example'
  check "field: v2 region" \
    '{"_kind":"cloud_resource_v2","hasSensitiveData":true,"region":"ap-southeast-9"}' \
    'ap-southeast-9'
  check "field: v2 code repository" \
    '{"_kind":"cloud_resource_v2","iacDetails":{"iacStatus":"DRIFTED"},"codeRepository":{"name":"example-corp/infra"}}' \
    'example-corp/infra'
  check "field: audit source IP" \
    '{"_kind":"audit_log_entry","actionType":"UPDATE","sourceIP":"203.0.113.9"}' \
    '203.0.113.9'
  check "field: audit performer identity" \
    '{"_kind":"audit_log_entry","actionType":"UPDATE","performer":{"name":"Jane Q Example"}}' \
    'Jane Q Example'
  check "field: audit action parameters" \
    '{"_kind":"audit_log_entry","actionType":"UPDATE","actionParameters":{"bucket":"example-corp-secrets"}}' \
    'example-corp-secrets'
  check "field: service account scopes" \
    '{"_kind":"service_account","enabled":true,"scopes":["read:issues","admin:audit"]}' \
    'admin:audit'
  check "field: issue assignee" \
    '{"_kind":"issue","statusChangedAt":"2026-08-01T00:00:00Z","assignee":{"name":"Jane Q Example"}}' \
    'Jane Q Example'
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
    'svc@example-corp.com' \
    text
  check "text: OCID" \
    'ocid1.compartment.oc1..aaaaaaaaexamplecompartmentid00000' \
    'ocid1.compartment.oc1..aaaaaaaaexamplecompartmentid00000' \
    text
  check "text: ARN" \
    'arn:aws:iam::123456789012:role/ExampleRole' \
    'arn:aws:iam::123456789012:role/ExampleRole' \
    text
  check "text: GCP resource path" \
    'projects/example-project-dev/serviceAccounts/x' \
    'projects/example-project-dev/serviceAccounts/x' \
    text
  check "text: azure subscription path" \
    '/subscriptions/00000000-1111-2222-3333-444444444444/resourceGroups/rg1' \
    '/subscriptions/00000000-1111-2222-3333-444444444444/resourceGroups/rg1' \
    text
  check "text: bare account id" \
    'account 123456789012 scanned' \
    '123456789012' \
    text
  check "text: GUID" \
    'id 00000000-1111-2222-3333-444444444444 seen' \
    '00000000-1111-2222-3333-444444444444' \
    text
  check "text: bearer token" \
    'Authorization: Bearer abcdefghij0123456789xyz' \
    'abcdefghij0123456789xyz' \
    text
  check "text: region hostname" \
    'POST https://api.us999.app.wiz.io/graphql' \
    'api.us999.app.wiz.io' \
    text
  check "text: IP address" \
    'peer 203.0.113.42 refused' \
    '203.0.113.42' \
    text

  # Shape dispatch. Measured 2026-08-06: the field layer engaged on one
  # of four shapes and fell through silently on the rest. These cases
  # exist so that cannot come back quietly.
  check "shape: json-array field-scrubbed" \
    '[
  {"_kind":"issue","severity":"HIGH","entitySnapshot":{"name":"Jane Q Example"}}
]' \
    'Jane Q Example'
  check "text: lowercase bearer" \
    'header: bearer abcdefghij0123456789xyz' \
    'abcdefghij0123456789xyz' \
    text

  refuses() { # name, input
    local name="$1" input="$2" rc=0
    printf '%s\n' "$input" | run_scrub >/dev/null 2>&1 || rc=$?
    if [[ "$rc" -eq 3 ]]; then
      printf 'ok   %-28s (refused)\n' "$name"
    else
      printf 'FAIL %-28s should have refused, exit %s\n' "$name" "$rc" >&2
      fail=1
    fi
  }
  refuses "shape: md-table refuses" '| _kind | id | severity |
|---|---|---|
| issue | prod-db-primary | HIGH |'
  refuses "shape: api output refuses" '{
  "issuesV2": {"nodes": [{"entitySnapshot": {"name": "prod-host"}}]}
}'
  refuses "shape: bare prose refuses" 'the resource prod-db-primary is exposed'

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

  # Null survives as null, and nothing else does. A null carries no
  # tenant data, and rendering it as "<redacted:field>" made absent and
  # populated byte-identical, which erased the only question the
  # live-validation queue asks. Found 2026-08-07 when the first real
  # validation call could not tell an unpopulated vulnerableAsset from a
  # populated one.
  local nulls
  nulls="$(printf '%s\n' '{"_kind":"issue","assignee":null,"sourceIP":null,"name":"box","projects":[],"scopes":"","performer":false,"entitySnapshot":{"name":null}}' | run_scrub)"
  if printf '%s' "$nulls" | grep -q '"assignee":null' \
    && printf '%s' "$nulls" | grep -q '"sourceIP":null'; then
    printf 'ok   %-28s\n' "null: absence survives as null"
  else
    printf 'FAIL %-28s null was not preserved: %s\n' "null: absence survives" "$nulls" >&2
    fail=1
  fi
  # The negative half, and the more important one: only a literal null
  # takes that path. An empty array, an empty string, false, and a real
  # string all stay redacted, because an array length or an
  # empty-versus-absent distinction on a real value is a signal about
  # the tenant. `entitySnapshot` is an ALLOWED container, so it is
  # descended into rather than redacted whole, and a null denied field
  # inside it stays null for the same reason it does at the top level.
  # The first draft of this test asserted entitySnapshot was redacted
  # whole, and the test caught the author rather than the scrubber.
  if printf '%s' "$nulls" | grep -q '"projects":"<redacted' \
    && printf '%s' "$nulls" | grep -q '"scopes":"<redacted' \
    && printf '%s' "$nulls" | grep -q '"performer":"<redacted' \
    && printf '%s' "$nulls" | grep -q '"name":"<redacted' \
    && ! printf '%s' "$nulls" | grep -q '"box"' \
    && printf '%s' "$nulls" | grep -q '"entitySnapshot":{"name":null}'; then
    printf 'ok   %-28s\n' "null: only null takes that path"
  else
    printf 'FAIL %-28s a non-null took the null path: %s\n' "null: only null" "$nulls" >&2
    fail=1
  fi

  # Second positive control, for the v2 additions specifically. If
  # these are redacted, `cloud_resource_v2` cannot answer a single
  # runbook question after scrubbing and the binding is decorative.
  local v2kept
  v2kept="$(printf '%s\n' '{"_kind":"cloud_resource_v2","isAccessibleFromInternet":true,"owners":[{"type":"DECLARED_OWNER"}],"issueAnalytics":{"criticalSeverityCount":3}}' | run_scrub)"
  if printf '%s' "$v2kept" | grep -q '"isAccessibleFromInternet":true' \
    && printf '%s' "$v2kept" | grep -q 'DECLARED_OWNER' \
    && printf '%s' "$v2kept" | grep -q '"criticalSeverityCount":3'; then
    printf 'ok   %-28s\n' "positive control: v2 kept"
  else
    printf 'FAIL %-28s v2 safe fields were destroyed: %s\n' "positive control v2" "$v2kept" >&2
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
