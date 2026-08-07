# Paper pipelines for the twenty operator runbooks

bd `aae-orc-e4jo.14`. Written 2026-08-07. The treatment arm of the verb
experiment.

**Nothing here was executed.** No `stave` invocation in this document was
run, no tenant was contacted, no API budget was spent. Every pipeline is
written as if about to be typed and then stopped.

Inputs: `docs/runbooks/catalogue.md`, `docs/runbooks/attemptability.md`,
`docs/design/field-surface-audit.md`, sections 1 to 3 of
`docs/design/verb-candidate-registration.md`, the curated operation
documents in `crates/stave-api/ops/`, `crates/stave-sdk/src/kinds.rs`,
`crates/stave-cli/src/main.rs`, and `.claude/rules/cli-philosophy.md`.
The sealed control arm and the gate-scoped amendments were not read.

---

## Conventions

**Stage.** One distinct analytical purpose. A `stave` invocation is a
stage; so is each `jq` step, each shell step, and each manual step.
Where one `jq` invocation would carry two purposes, it is written as two
stages, because the purposes are what gets tallied.

**Purpose.** One phrase naming what the stage does to the data, not how.
The purposes are the evidence; the `jq` text is illustrative and is kept
short.

**Survives.** Would this stage still exist if all four read-surface
tickets had landed: `aae-orc-rsh6` (bind `cloudResourcesV2`),
`aae-orc-j1xi` (declare `filterBy` and `orderBy`), `aae-orc-qijl` (widen
field selections), `aae-orc-gs23` (bind the aggregation, history, and
diff roots). `yes` means the stage is real work. `no` means the stage
exists only because a field is unselected or a root unbound, and the
tally must not read it as verb demand.

**Hygiene stages are excluded from every tally.** Every pipeline pipes
tenant output through `scripts/scrub.sh` per
`.claude/rules/tenant-data-hygiene.md`. It is shown once per pipeline and
never counted, because it is a safety control and not analysis. Likewise
every `stave` invocation shown would go to the `stave-safety-coach`
subagent first per `.claude/rules/safety-coach-gate.md`; that gate is
assumed throughout and not repeated per stage.

**Blocked steps** are written as `BLOCKED` rows with what they would
need. A blocked step is not a glue stage and is not counted in either
tally, but what it would need is recorded because it is evidence.

**[SIMULATE]** rows produce the artifact an action would have produced
and call nothing. They are counted as stages because they are real work
in the pipeline.

**Gap marks.** Where a registered verb would be used if it existed, the
stage carries the verb id in the gap column. Marks were made for all
twelve registered entries deliberately and per runbook, priors and
decoys alike, rather than only where a prior happened to fit.

---

## Class A: Queries

### A1. Remediation SLA sweep

```sh
stave list issue --limit 5000 | scripts/scrub.sh > issues.jsonl
stave filter --where 'status == "OPEN"' < issues.jsonl > open.jsonl
jq -c '. + {age_days: ((now - (.createdAt|fromdate)) / 86400 | floor)}' open.jsonl \
  | jq -c --argjson sla '{"CRITICAL":7,"HIGH":30,"MEDIUM":90,"LOW":180}' \
      '. + {past_sla: (.age_days > ($sla[.severity] // 180))}' \
  | jq -c '. + {band: (if .age_days<8 then "0-7" elif .age_days<31 then "8-30" else "31+" end)}' \
  > aged.jsonl
jq -s 'group_by([.severity,.status,.band]) | map({k:.[0], n:length})' aged.jsonl
jq -s 'sort_by(-(.age_days - $window)) | .[]' aged.jsonl
jq -s '[.[] | select(.past_sla)] | length' aged.jsonl
stave emit --format md < aged.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list issue` | n/a | yes | |
| 2 | [stave] `filter --where status == OPEN` | n/a | yes (becomes a server filter under j1xi, still one invocation) | |
| 3 | jq group_by severity, status | group and count on a composite key | **no** (gs23 `issuesGroupedByValue`) | P2, D3 |
| 4 | jq derive `age_days` | derive a per-record field from a timestamp | yes | |
| 5 | jq apply the SLA table | derive a flag from an inline policy table | yes | D5 |
| 6 | jq band and cross-tabulate | reshape into a two-key matrix on a derived key | yes | P2, D3 |
| 7 | jq sort by overdue margin | order a stream by a derived key | yes | D1 |
| 8 | jq count past-SLA rows | count a subset | yes | |
| 9 | jq join issues to users for a contact | match two streams on a key | **no** (qijl selects `assignee { name email }` inline) | P1 |
| 10 | [stave] `emit --format md` | n/a | yes | |

BLOCKED: step 4 of the runbook, owner attribution. Needs
`Issue.assignee` (qijl) or `issueSuggestedAssignees` (gs23). This is the
step the runbook exists for, so today the pipeline produces three
quarters of an artifact and no answer to "who am I chasing".

RAW glue 7, POST-FIX 5.

### A2. Emergency blast radius

```sh
stave search vulnerability_finding 'CVE-2026-XXXXX' --limit 5000 | scripts/scrub.sh > vf.jsonl
stave list cloud_resource --limit 50000 | scripts/scrub.sh > res.jsonl
jq -c --slurpfile r res.jsonl '($r|INDEX(.id)) as $i | . + {resource: $i[.assetId]}' vf.jsonl > joined.jsonl
jq -s 'map(.resource.id) | unique | length' joined.jsonl
jq -s 'group_by([.resource.subscriptionExternalId, .resource.type]) | map({k:.[0],n:length})' joined.jsonl
stave filter --where 'resource.isAccessibleFromInternet' < joined.jsonl > exposed.jsonl
jq -s 'length' exposed.jsonl
jq -c '.resource.owners[]' exposed.jsonl | jq -s 'unique'
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `search vulnerability_finding` | n/a | yes | |
| 2 | jq match a CVE id against the finding name string | reshape a name into a comparable key | **no** (qijl selects `vulnerabilityExternalId`) | |
| 3 | [stave] `list cloud_resource` (V2 under rsh6) | n/a | yes | |
| 4 | jq join findings to resources on asset id | match two streams on a key | yes (exposure lives on `CloudResourceV2`, not on the finding) | P1 |
| 5 | jq distinct resources carrying the CVE | collapse to one record per key | yes | D2 |
| 6 | jq group by account and resource type | group and count on a composite key | **no** (gs23 `vulnerabilityFindingsGroupedByValues`) | P2, D3 |
| 7 | [stave] `filter` on the joined exposure flag | n/a | yes | |
| 8 | jq count the exposed subset | count a subset after a join | yes | |
| 9 | jq collect owner contacts | match owner references to people | **no** (rsh6 `owners` arrives with the resource) | P1 |
| 10 | jq sort by account exposure count | order a stream by a derived key | yes | D1 |

BLOCKED today at every step: `vulnerabilityExternalId`,
`vulnerableAsset`, and the V2 exposure fields are all out of reach, so
step 1 of the runbook (does the CVE exist in the estate) cannot be
answered except by substring-matching finding names.

Note on the twenty-minute criterion: `search` is a full-connection walk
by construction, and the safety coach halts on it. Even with the walk
accepted, two full connections must be materialised before the first
join. Under j1xi the finding side becomes a server-side filter and the
resource side stays a walk.

RAW glue 7, POST-FIX 4.

### A3. Toxic combination triage

```sh
stave list issue --limit 5000 | scripts/scrub.sh > iss.jsonl
stave filter --where 'severity in ["CRITICAL","HIGH"]' < iss.jsonl > sev.jsonl
stave list cloud_resource --limit 50000 | scripts/scrub.sh > res.jsonl
jq -c --slurpfile r res.jsonl '($r|INDEX(.id)) as $i | . + {res: $i[.entitySnapshot.id]}' sev.jsonl > j.jsonl
stave filter --where 'res.isAccessibleFromInternet && res.hasSensitiveData' < j.jsonl > toxic.jsonl
jq -s 'sort_by([-(.res.issueAnalytics.criticalCount), .severity]) | .[0:25] | .[]' toxic.jsonl
stave emit --format md
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list issue` | n/a | yes | |
| 2 | [stave] `filter` on severity | n/a | yes | |
| 3 | [stave] `list cloud_resource` (V2) | n/a | yes | |
| 4 | jq join issues to resources on entity id | match two streams on a key | yes (per the audit, exposure and sensitivity are V2-only, so they arrive on a separate stream) | P1 |
| 5 | [stave] `filter` on two joined booleans | n/a | yes | |
| 6 | jq sort by a joined analytics count | order a stream by a joined key | yes | D1, D4 |
| 7 | jq take the top 25 | take a subset after ranking | yes | D4 |
| 8 | jq count by severity for the header | group and count | **no** (gs23) | P2 |

BLOCKED today at steps 2 and 3 of the runbook: both narrowings are
V2-only, so the pipeline stops after the severity filter, which is the
part the runbook explicitly says is not the answer.

RAW glue 4, POST-FIX 3.

### A4. Standing credential review

```sh
stave list service_account --limit 500 | scripts/scrub.sh > sa.jsonl
jq -c '. + {age_days: ((now - (.createdAt|fromdate))/86400|floor)}' sa.jsonl \
  | jq -c '. + {band: (if .age_days<90 then "<90" elif .age_days<365 then "90-365" else "365+" end)}' > aged.jsonl
jq -s 'group_by(.band) | map({band:.[0].band, n:length})' aged.jsonl
stave list audit_log --since 2160h --limit 50000 | scripts/scrub.sh > al.jsonl
jq -c '{actor: .serviceAccount.id, ts: .timestamp}' al.jsonl \
  | jq -s 'group_by(.actor) | map({actor:.[0].actor, last: (max_by(.ts).ts)})' > last.jsonl
jq -c --slurpfile l last.jsonl '($l[0]|INDEX(.actor)) as $i | . + {last_seen: $i[.id].last}' aged.jsonl \
  | jq -c 'select(.last_seen == null)'
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list service_account` | n/a | yes | |
| 2 | jq derive age and band | derive a field and bucket it | yes | |
| 3 | jq count per band | group and count on a derived key | yes (no grouped root for this kind, and the key is derived) | P2 |
| 4 | [stave] `list audit_log --since` | n/a | **no** (see below) | |
| 5 | jq join audit entries to accounts on actor id | match two streams on a key | **no** | P1 |
| 6 | jq reduce to last activity per account | take a per-group extreme | **no** | P2 |
| 7 | jq accounts with no matching entry | presence on one side only | **no** | P1 |

The four `no` rows are the sharpest instance in the catalogue of glue
that is document debt. The runbook's criterion is "no activity in the
window", and `ServiceAccount.lastLoginAt` plus `lastRotatedAt` and
`enabled` (all qijl) answer it as a field comparison on the stream
already in hand. The entire audit-log join disappears. What survives the
fix is a narrower residual: `lastLoginAt` covers interactive login and
not every API use, so an operator who means "used the API at all" still
needs the join. That residual is real but it is not what the runbook
asked for, and counting the join as verb demand would be counting a
one-line document edit as a missing verb.

BLOCKED today at steps 3 and 4 of the runbook: the audit log carries no
actor in the current selection, so the join has nothing to join on.

RAW glue 5, POST-FIX 2.

### A5. Framework evidence pull

```sh
stave list security_framework --limit 200 | scripts/scrub.sh > fw.jsonl
stave list control --limit 2000 | scripts/scrub.sh > ctl.jsonl
jq -c --slurpfile f fw.jsonl '($f|INDEX(.id)) as $i | . + {framework: $i[.frameworkId].name}' ctl.jsonl > j.jsonl
jq -s 'group_by([.framework,.severity]) | map({k:.[0], total:length, on:([.[]|select(.enabled)]|length), pct:(([.[]|select(.enabled)]|length)/length)})' j.jsonl
printf '%s\n' "as_of=$(date -u +%FT%TZ)" "ops=list_security_frameworks,list_controls" \
  "schema_sha=$(cat spec/wiz-schema.graphql.sha256)" "stave=$(stave --version)"
diff <(jq -S . coverage-2026-05-01.json) <(jq -S . coverage-today.json)
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list security_framework` | n/a | yes | |
| 2 | [stave] `list control` | n/a | yes | |
| 3 | jq join controls to frameworks | match two streams on a key | **no** (qijl selects `SecurityFramework.controls` nested) | P1 |
| 4 | jq coverage by framework and severity | group, count, and derive a ratio per group | yes (no controls grouped root; the ratio is derived) | P2, D3 |
| 5 | shell stamp as-of, operations, schema sha, version | record the derivation for the artifact | yes | D5 |
| 6 | diff against the archived prior quarter | compare one stream at two times | yes | D7 |

BLOCKED today at step 2 of the runbook: `SecurityFramework.controls` is
unselected, so the framework roster has no controls under it and the
join in stage 3 has no key. `stave list control` returns controls with
no framework linkage at all.

Stage 5 is worth calling out. The audit trail already records the
operation, the variables, the timestamp, and the endpoint per call, so
the material for "how derived" exists and the glue is extraction. The
success criterion "as of when, how derived, show me last quarter's" is
three questions and stave answers none of them as a first-class output.

RAW glue 4, POST-FIX 3.

---

## Class B: Joins

Every runbook in this class needs a stream the security graph does not
hold. The external half of each pipeline is unaffected by all four
tickets, which is why this class barely moves between the two tallies.

### B6. Join key coverage

```sh
stave list cloud_resource --limit 50000 | scripts/scrub.sh > res.jsonl
stave list cloud_account --limit 500 | scripts/scrub.sh > acc.jsonl
# external: cmdb.jsonl cost.jsonl tickets.jsonl roster.jsonl, normalised to JSONL by hand
jq -c '{externalId, providerUniqueId, region, arn: .tags.arn, cmdb_id: .tags["cmdb-ci"]}' res.jsonl > keys.jsonl
jq -s 'reduce .[] as $r ({}; reduce (keys_unsorted[]) as $k (.; .[$k] += (if $r[$k] then 1 else 0 end)))' keys.jsonl
jq -c '. + {norm_arn: (.arn // "" | ascii_downcase | sub("^arn:aws:";""))}' keys.jsonl > nk.jsonl
jq -s --slurpfile c cmdb.jsonl '($c|map(.ci_arn|ascii_downcase)|unique) as $r | [.[]|select(.norm_arn|IN($r[]))] | length' nk.jsonl
jq -s 'reduce .[] as $p ({}; .[$p.left][$p.right] = $p.pct)' pairs.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list cloud_resource` (V2) | n/a | yes | |
| 2 | [stave] `list cloud_account` | n/a | yes | |
| 3 | shell load four external extracts | n/a | yes | |
| 4 | jq per-key null rate per stream | compute a population fraction per field | yes | P2, P3 |
| 5 | jq normalise each candidate key | reshape a key into a comparable form | yes | |
| 6 | jq per key pair, fraction of left keys matching a right key | measure correspondence between two streams on a key | yes | P3 |
| 7 | jq build the system-by-key matrix | reshape into a matrix keyed by system pair | yes | D3 |
| 8 | jq sort pairs by correspondence | order a stream by a derived key | yes | D1 |

BLOCKED today at steps 1 and 2 of the runbook for the graph side:
`externalId`, `providerUniqueId`, `region`, and `tags` are all
`CloudResourceV2`, so the graph half of the key inventory cannot be
measured. The external half is measurable today with no stave
involvement at all, which is worth stating: for this runbook stave is
currently the weakest of the five systems being measured.

RAW glue 5, POST-FIX 5.

### B7. CMDB three-bucket reconciliation

```sh
stave list cloud_resource --limit 50000 | scripts/scrub.sh > res.jsonl
jq -s 'group_by(.ci_id) | map(max_by(.updated_at)) | .[]' cmdb.jsonl > cmdb1.jsonl
jq -c '. + {k: (.tags["cmdb-ci"] // .externalId | ascii_downcase)}' res.jsonl > rk.jsonl
jq -c '. + {k: (.ci_id | ascii_downcase)}' cmdb1.jsonl > ck.jsonl
jq -c --slurpfile c ck.jsonl '($c|INDEX(.k)) as $i | select($i[.k] == null)' rk.jsonl   # bucket 1
jq -c --slurpfile r rk.jsonl '($r|INDEX(.k)) as $i | select($i[.k] == null)' ck.jsonl   # bucket 2
jq -c --slurpfile c ck.jsonl '($c|INDEX(.k)) as $i | $i[.k] as $m | select($m)
  | {k, disagree: ([{f:"owner",a:.owners[0].email,b:$m.owner},
                    {f:"env",a:.tags.env,b:$m.environment},
                    {f:"crit",a:.tags.criticality,b:$m.criticality}]
                   | map(select(.a != .b)))} | select(.disagree|length>0)' rk.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list cloud_resource` (V2) | n/a | yes | |
| 2 | shell load the CMDB extract | n/a | yes | |
| 3 | jq collapse duplicate CMDB records per CI | collapse to one record per key with a pick rule | yes | D2 |
| 4 | jq normalise the join key on both sides | reshape a key into a comparable form | yes | P3 |
| 5 | jq keys in the cloud, not in the CMDB | presence on one side only | yes | P1, P5 |
| 6 | jq keys in the CMDB, not in the cloud | presence on the other side only | yes | P1, P5 |
| 7 | jq matched pairs, per-field disagreement | match on a key then report per-field disagreement | yes | P1, P4, P5 |
| 8 | jq count per bucket and per disagreeing attribute | group and count | yes (post-join) | P2, P5, D3 |

Stage 3 matters more than it looks. CMDB extracts routinely carry
several rows per CI, and without the collapse the inner join in stage 7
fans out and every count in stage 8 is wrong in a way that looks
plausible.

RAW glue 6, POST-FIX 6.

### B8. Ownerless-resource cross-check

```sh
stave list cloud_resource --limit 50000 | scripts/scrub.sh > res.jsonl
stave filter --where 'size(owners) == 0 || !has(tags.owner)' < res.jsonl > noowner.jsonl
jq -c --slurpfile t teams.jsonl '($t|INDEX(.name)) as $i | . + {team_alive: ($i[.tags.owner].status == "active")}' res.jsonl \
  | jq -c 'select(.team_alive != true)' > orphan.jsonl
stave list issue --limit 5000 | scripts/scrub.sh > iss.jsonl
jq -c --slurpfile o orphan.jsonl '($o|INDEX(.id)) as $i | select($i[.entitySnapshot.id])' iss.jsonl > oi.jsonl
jq -c --slurpfile c cost.jsonl '($c|INDEX(.resource_id)) as $i | . + {cost: $i[.id].monthly_usd}' orphan.jsonl > oc.jsonl
jq -s '{orphan: length, with_issues: ..., with_cost: ..., all_three: ...}' 
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list cloud_resource` (V2) | n/a | yes | |
| 2 | [stave] `filter` on missing owner | n/a | yes | |
| 3 | shell load the team registry and the cost extract | n/a | yes | |
| 4 | jq join resources to the team registry, flag dissolved | match against an external set on a key | yes | P1, P3 |
| 5 | jq join orphan resources to issues on entity id | match two streams on a key | yes | P1 |
| 6 | jq join orphan resources to cost on resource id | match against an external set on a key | yes | P1 |
| 7 | jq three-way membership overlap | compare membership across three key sets | yes | P1 (see note) |
| 8 | jq sum cost and issue counts per segment | group and aggregate per segment | yes (post-join) | P2 |
| 9 | jq sort segments by size | order by a derived key | yes | D1 |

Note on stage 7. `join` as registered takes two streams. A three-way
overlap needs it applied twice with an intermediate, or an arity
extension. Recording that as a fact about the registered argument shape
rather than resolving it here.

RAW glue 6, POST-FIX 6.

### B9. Control assertion reconciliation

```sh
stave list control --limit 2000 | scripts/scrub.sh > ctl.jsonl
# external: grc.jsonl, the asserted control register
jq -c '. + {k: (.name | ascii_downcase | gsub("[^a-z0-9]";""))}' ctl.jsonl > ck.jsonl
jq -c '. + {k: (.control_ref | ascii_downcase | gsub("[^a-z0-9]";""))}' grc.jsonl > gk.jsonl
jq -c --slurpfile c ck.jsonl '($c|INDEX(.k)) as $i | select($i[.k]==null)' gk.jsonl
jq -c --slurpfile g gk.jsonl '($g|INDEX(.k)) as $i | select($i[.k]==null)' ck.jsonl
jq -c --slurpfile c ck.jsonl '($c|INDEX(.k)) as $i | $i[.k] as $m | select($m)
  | select(.asserted_state != (if $m.enabled then "implemented" else "not-implemented" end))' gk.jsonl
jq -c 'select(.enabled and (.lastSuccessfulRunAt == null or (.lastSuccessfulRunAt|fromdate) < (now - 2592000)))' ck.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list control` | n/a | yes | |
| 2 | shell load the GRC register | n/a | yes | |
| 3 | jq normalise control identifiers across two vocabularies | reshape a key into a comparable form | yes | P3 |
| 4 | jq asserted but not present in the graph | presence on one side only | yes | P1, P5 |
| 5 | jq present in the graph but not asserted | presence on the other side only | yes | P1, P5 |
| 6 | jq matched pairs where the state disagrees | per-field disagreement across matched records | yes | P1, P4, P5 |
| 7 | jq enabled but never or not recently run | derive a substantiation verdict from two fields | yes | |
| 8 | jq count the unsubstantiated set | group and count | yes | P2, P5 |

`securityFrameworksDiff` (gs23) does not serve this runbook. It compares
frameworks to frameworks inside Wiz; B9 compares Wiz to an external
register, and no server root crosses that boundary. Recording this
because the audit lists the diff root against B9 and the two are not the
same comparison.

BLOCKED today at the substantiation half: `lastSuccessfulRunAt` is
unselected, so stage 7 has nothing to test and stage 4's list cannot be
narrowed to assertions that are unevidenced rather than merely absent.

RAW glue 6, POST-FIX 6.

### B10. Change drift reconciliation

```sh
stave list audit_log --since 720h --limit 50000 | scripts/scrub.sh > al.jsonl
stave filter --where 'actionType in ["UPDATE_CONTROL","UPDATE_CLOUD_CONFIG_RULE"]' < al.jsonl > chg.jsonl
# external: changes.jsonl, the change management export
jq -c --slurpfile c changes.jsonl '. as $e | ($c | map(select(
     (.object_ref == $e.actionParameters.id)
     and ((.window_start|fromdate) <= ($e.timestamp|fromdate))
     and ((.window_end|fromdate)   >= ($e.timestamp|fromdate))))) as $m
   | . + {approval: ($m[0] // null)}' chg.jsonl > matched.jsonl
jq -c 'select(.approval == null)' matched.jsonl
jq -s 'group_by(.actionParameters.id) | map({control:.[0].actionParameters.id,
   intervals: (sort_by(.timestamp) | [.[] | {t:.timestamp, on:.actionParameters.enabled}])})' chg.jsonl
jq -s 'map(select(.intervals | any(.on == false)))'
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list audit_log --since` | n/a | yes | |
| 2 | [stave] `filter` on action type | n/a | yes | |
| 3 | shell load the change management export | n/a | yes | |
| 4 | jq match events to change records on object and time window | match two streams where no exact key exists | yes | P1 (see note), P3 |
| 5 | jq events with no approving record | presence on one side only | yes | P1, P5 |
| 6 | jq reconstruct per-control enable and disable intervals | build state intervals from an event sequence | yes | |
| 7 | jq controls disabled at any point in the period | compare a control's state at two times | **no** (gs23 binds `issueHistoryEvents` and the trend roots) | D7 |
| 8 | jq count unapproved changes | group and count | yes | P2 |

Note on stage 4. This is the one join in the catalogue where the key
itself is the problem. There is no shared identifier between an audit
event and a change ticket; correspondence is object identity plus a time
window. A `join` verb whose argument shape is a key expression does not
serve it. Stage 6 is also worth separating from stage 7: interval
reconstruction from an event sequence is not two-point comparison, and a
`diff --since` verb does not absorb it.

BLOCKED today at every step: the audit log selection carries no
`performer`, `actionType`, or `actionParameters`, so there is nothing to
filter on and nothing to match against.

RAW glue 5, POST-FIX 4.

### B11. Scan coverage gap

The one runbook whose graph half is served today. If exactly one
synthetic fixture is ever built, this is the runbook that yields a real
attempt.

```sh
stave list cloud_account --limit 500 | scripts/scrub.sh > acc.jsonl
# external: roster.jsonl from the vending pipeline or the billing hierarchy
jq -c '. + {k: (.externalId | ascii_downcase | ltrimstr("account/"))}' acc.jsonl > ak.jsonl
jq -c '. + {k: (.account_number | tostring | ascii_downcase)}' roster.jsonl > rk.jsonl
jq -c --slurpfile a ak.jsonl '($a|INDEX(.k)) as $i | select($i[.k] == null)' rk.jsonl > unscanned.jsonl
jq -c '. + {days_unscanned: ((now - (.vended_at|fromdate))/86400|floor)}' unscanned.jsonl \
  | jq -s 'sort_by(-.days_unscanned) | .[]'
stave emit --format md
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list cloud_account` | n/a | yes | |
| 2 | shell load the roster | n/a | yes | |
| 3 | jq normalise account identifiers on both sides | reshape a key into a comparable form | yes | P3 |
| 4 | jq roster accounts with no scanner account | presence on one side only | yes | P1, P5 |
| 5 | jq derive days unscanned from the vend date | derive a duration | yes | |
| 6 | jq sort by days unscanned | order by a derived key | yes | D1, D4 |

Stage 6 is the clearest case that `orderBy` (j1xi) does not absorb a
sort. The rows being sorted are the ones absent from the connection, so
no server-side ordering of that connection can reach them.

RAW glue 4, POST-FIX 4.

### B12. Ticket reconciliation

```sh
stave list issue --limit 5000 | scripts/scrub.sh > iss.jsonl
# external: tickets.jsonl
jq -c '. + {k: (.serviceTickets[0].externalId // .id)}' iss.jsonl > ik.jsonl
jq -s 'group_by(.issue_ref) | map({ref:.[0].issue_ref, n:length, first:(min_by(.created).created)})' tickets.jsonl > tg.jsonl
jq -c --slurpfile t tg.jsonl '($t[0]|INDEX(.ref)) as $i | select(.status=="OPEN" and $i[.k]==null)' ik.jsonl
jq -c --slurpfile i ik.jsonl '($i|INDEX(.k)) as $x | select(.status=="closed" and $x[.issue_ref].status=="OPEN")' tickets.jsonl
jq -c --slurpfile t tg.jsonl '($t[0]|INDEX(.ref)) as $i | $i[.k] as $m | select($m)
  | {k, issue_age: ..., ticket_age: ..., delta: ...}' ik.jsonl
jq -s '[.[]|select(.n>1)] | length' tg.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list issue` | n/a | yes | |
| 2 | shell load the ticketing export | n/a | yes | |
| 3 | jq normalise the issue to ticket key | reshape a key into a comparable form | yes | P3 |
| 4 | jq collapse to one ticket per issue with a pick rule | collapse to one record per key | yes | D2 |
| 5 | jq open issues with no ticket | presence on one side only | **no** (j1xi passes `hasServiceTicket` as a server-side boolean) | P1, P5 |
| 6 | jq closed tickets whose issue is still open | presence and status mismatch across two streams | yes | P1, P5 |
| 7 | jq compare ticket age to issue age on matched pairs | per-field comparison across matched records | yes | P4, P5 |
| 8 | jq count tickets per issue and flag duplicates | group and count per key | yes | P2 |
| 9 | jq derive the false-closure rate | derive a ratio from two counts | yes | |

Stage 5 is the second clean instance of document debt reading as verb
demand. Step 1 of the runbook is an anti-join today and a filter
argument after j1xi.

RAW glue 7, POST-FIX 6.

### B13. Decommission verification

```sh
stave list cloud_resource --limit 50000 | scripts/scrub.sh > res.jsonl
stave list cloud_account --limit 500 | scripts/scrub.sh > acc.jsonl
# external: cmdb-retired.jsonl
jq -c '. + {k: (.tags["cmdb-ci"] // .externalId | ascii_downcase)}' res.jsonl > rk.jsonl
jq -c --slurpfile r rk.jsonl '($r|INDEX(.k)) as $i | $i[.ci_id|ascii_downcase] as $m
   | select($m and $m.deletedAt == null and $m.status != "Inactive")' cmdb-retired.jsonl
jq -c --slurpfile c cmdb-retired.jsonl '($c|INDEX(.ci_id|ascii_downcase)) as $i
   | select(.deletedAt != null and ($i[.k].state // "active") != "retired")' rk.jsonl
jq -c --slurpfile a closed-accounts.jsonl '($a|INDEX(.account_number)) as $i
   | select($i[.externalId] and .connector != null)' acc.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list cloud_resource` (V2) | n/a | yes | |
| 2 | [stave] `list cloud_account` | n/a | yes | |
| 3 | shell load the retired-record extract | n/a | yes | |
| 4 | jq normalise keys on both sides | reshape a key into a comparable form | yes | P3 |
| 5 | jq retired in the CMDB but live in the cloud | presence on one side with a state predicate | yes | P1, P5 |
| 6 | jq gone from the cloud but active in the CMDB | presence on the other side with a state predicate | yes | P1, P5 |
| 7 | jq accounts recorded closed that still have a connector | contradiction across a joined pair | yes | P1, P4, P5 |
| 8 | jq count per direction | group and count | yes | P2, P5 |

BLOCKED today at steps 2 and 4 of the runbook: `deletedAt`, `lastSeen`,
and `status` are V2-only, and `CloudAccount.status` and `connector` are
unselected, so neither direction of the contradiction can be evaluated.

RAW glue 5, POST-FIX 5.

---

## Class C: Spoke-team runbooks

### C14. Root-cause collapse

```sh
stave list vulnerability_finding --limit 50000 | scripts/scrub.sh > vf.jsonl
jq -c '. + {cause: (.layerMetadata.digest // .rootComponent.name + "@" + .rootComponent.version
                    // .vulnerableAsset.imageId // "unknown")}' vf.jsonl > caused.jsonl
jq -s 'group_by(.cause) | map({cause:.[0].cause, n:length,
        worst:( [.[].severity] | map({CRITICAL:4,HIGH:3,MEDIUM:2,LOW:1}[.]) | max)})' caused.jsonl > ranked.jsonl
jq -s 'sort_by([-.worst, -.n]) | .[0:20] | .[]' ranked.jsonl
jq -c '{would_file: {title: ("Fix " + .cause), instances: .n}}' ranked.jsonl > simulated-tickets.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list vulnerability_finding` | n/a | yes | |
| 2 | jq derive a cause key from layer, component, or image | derive a grouping key from several fields | yes | |
| 3 | jq group by cause, count instances, keep worst severity | group and compute per-group aggregates | yes (see note) | P2 |
| 4 | jq sort causes by severity then instance count | order by a derived key | yes | D1, D4 |
| 5 | jq take the top 20 | take a subset after ranking | yes | D4 |
| 6 | **[SIMULATE]** render the tickets that would be filed | produce an artifact without performing the action | yes | |

Note on stage 3. `vulnerabilityFindingsGroupedByLayer` (gs23) takes a
`resourceId` and groups one resource's findings by layer. It does not
group the estate by cause, and the cause key here is a client-side
choice across three candidate fields. The grouping survives the fix.
This is the counter-example to A1 and A2, where the grouping does not.

BLOCKED today at steps 1 and 3 of the runbook: `layerMetadata`,
`rootComponent`, and `severity` are unselected, so the cause key has no
inputs and the ranking has no severity.

RAW glue 5, POST-FIX 5.

### C15. Fix-at-source mapping

```sh
stave list vulnerability_finding --limit 50000 | scripts/scrub.sh > vf.jsonl
stave list cloud_resource --limit 50000 | scripts/scrub.sh > res.jsonl
jq -c --slurpfile r res.jsonl '($r|INDEX(.id)) as $i | . + {res: $i[.vulnerableAsset.id]}' vf.jsonl > j.jsonl
jq -c '. + {artifact: (.res.iacModuleSource // (.res.tags.image // "" | split("@")[0]))}' j.jsonl > a.jsonl
jq -c --slurpfile reg registry.jsonl '($reg|INDEX(.repository + ":" + .tag)) as $i
   | . + {source: $i[.artifact]}' a.jsonl > src.jsonl
jq -s 'group_by(.source.repo_url) | map({repo:.[0].source.repo_url, commit:.[0].source.commit,
        artifacts:( [.[].artifact]|unique ), findings:length})' src.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list vulnerability_finding` | n/a | yes | |
| 2 | [stave] `list cloud_resource` (V2) | n/a | yes | |
| 3 | jq join findings to resources on asset id | match two streams on a key | yes | P1 |
| 4 | shell load the IaC inventory and the registry manifest | n/a | yes | |
| 5 | jq normalise image tag, digest, and module source into one artifact key | reshape a key into a comparable form | yes | P3 |
| 6 | jq join to the registry and IaC inventory on the artifact key | match against an external set | yes | P1 |
| 7 | jq group findings by repository and commit | group and aggregate on a derived key | yes | P2 |
| 8 | **[SIMULATE]** render the proposed change per artifact | produce an artifact without performing the action | yes | |

The success criterion, "no new resources created from the vulnerable
artifact after date X", is a temporal check the pipeline cannot make
today and can only make after the fix through the history roots. Noted
rather than pipelined, because the runbook does not list it as a step.

RAW glue 5, POST-FIX 5.

### C16. Account enrollment lifecycle

```sh
stave list cloud_account --limit 500 | scripts/scrub.sh > acc.jsonl
# external: vending.jsonl
jq -c --slurpfile a acc.jsonl '($a|INDEX(.externalId)) as $i | . + {scanned: $i[.account_number].firstScannedAt}' vending.jsonl > w.jsonl
jq -c 'select(.scanned) | . + {window_h: (((.scanned|fromdate) - (.vended_at|fromdate))/3600|floor)}' w.jsonl > win.jsonl
jq -s 'sort_by(.window_h) | {n:length, p50:.[length/2|floor].window_h, p95:.[length*0.95|floor].window_h, max:(max_by(.window_h).window_h)}' win.jsonl
jq -c 'select(.scanned == null)' w.jsonl
jq -s 'sort_by(-.window_h) | .[0:10] | .[]' win.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list cloud_account` | n/a | yes | |
| 2 | shell load the vending pipeline records | n/a | yes | |
| 3 | jq join vend records to scanner accounts | match against an external set on a key | yes | P1, P3 |
| 4 | jq derive the window across the joined pair | derive a duration from two records | yes | |
| 5 | jq distribution, percentiles, and maximum | aggregate a derived numeric across a stream | yes | P2 |
| 6 | jq vended accounts with no scanner record | presence on one side only | yes | P1, P5 |
| 7 | jq worst ten by window | order and take a subset | yes | D1, D4 |

BLOCKED today at step 2 of the runbook: `firstScannedAt` is unselected,
so even the scanner-side timestamp is out of reach and stage 3 has
nothing to attach.

RAW glue 5, POST-FIX 5.

### C17. Regression and recurrence tracking

Blocked entirely by our own read surface, with no external input needed.
The purest case in the catalogue.

```sh
stave list issue --limit 5000 | scripts/scrub.sh > today.jsonl
# archived by a prior run: snapshots/2026-07-07.jsonl
jq -c '. + {sig: ((.sourceRule.id // .type) + "|" + (.entitySnapshot.type // "") + "|" + (.rootComponent.name // ""))}' today.jsonl > s-now.jsonl
jq -s 'group_by(.sig) | map({sig:.[0].sig, first:(min_by(.createdAt).createdAt),
        last:(max_by(.updatedAt).updatedAt), entities:([.[].entitySnapshot.id]|unique)})' s-now.jsonl > n.jsonl
jq -s --slurpfile p snapshots/2026-07-07.sig.jsonl '($p[0]|INDEX(.sig)) as $i
   | [.[] | select($i[.sig] == null or $i[.sig].state == "closed")] | .[]' n.jsonl > returned.jsonl
jq -c --slurpfile p snapshots/2026-07-07.sig.jsonl '($p[0]|INDEX(.sig)) as $i
   | . + {same_entity: ((.entities - $i[.sig].entities) | length == 0)}' returned.jsonl
jq -s '{recurred: ([.[]|select(.returned)]|length), total: length, rate: ...}'
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list issue` | n/a | yes | |
| 2 | jq derive a resource-independent signature | derive a key from several fields | yes | |
| 3 | jq collapse to distinct signatures with first and last seen | collapse to one record per key with extremes | yes | D2, P2 |
| 4 | shell keep a dated snapshot per run | retain prior state stave does not hold | **no** (gs23 binds `issueHistoryEvents` and `issuesTrendV2`) | |
| 5 | jq compare the signature set now against the snapshot | compare the same key set at two times | yes (no trend root groups by a client-derived signature) | D7, P4 |
| 6 | jq split same-entity from new-entity reappearance | compare a field across two occurrences of one key | yes | P4, D7 |
| 7 | jq recurrence rate per cause | derive a ratio per group | yes | P2 |

Stage 4 deserves its own note. `diff --since` as registered takes "one
stream at two times", and stave holds no history at all: there is no
snapshot store, and today the second time point exists only if an
operator happened to keep yesterday's output. The verb presupposes state
the tool does not have. After gs23 the history roots supply it for
native fields, but not for a signature the operator derived.

RAW glue 6, POST-FIX 5.

### C18. Resolved versus evaporated

```sh
stave list issue --since 720h --limit 5000 | scripts/scrub.sh > iss.jsonl
stave filter --where 'status == "RESOLVED"' < iss.jsonl > res.jsonl
stave list cloud_resource --limit 50000 | scripts/scrub.sh > cr.jsonl
jq -c --slurpfile c cr.jsonl '($c|INDEX(.id)) as $i | . + {ent: $i[.entitySnapshot.id]}' res.jsonl > j.jsonl
jq -c '. + {outcome: (if .ent == null or .ent.deletedAt != null then "evaporated" else "remediated" end)}' j.jsonl > o.jsonl
jq -s '{reported: (length), corrected: ([.[]|select(.outcome=="remediated")]|length),
        gap_pct: ...}' o.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list issue --since` | n/a | yes | |
| 2 | [stave] `filter` on resolved | n/a | yes | |
| 3 | [stave] `list cloud_resource` (V2) | n/a | **no** | |
| 4 | jq join resolved issues to resources on entity id | match two streams on a key | **no** | P1 |
| 5 | jq classify remediated versus evaporated | derive a classification from joined fields | **no** | |
| 6 | jq recompute the metric with evaporated removed and report both | derive two aggregates over a partitioned stream | yes | P2 |

Three of six stages die, and the two that die are the ones that look
most like verb demand. `Issue.resolutionReason` and `resolvedBy` (qijl)
name the outcome directly, which is what the audit's C18.3 row says.
Under the raw tally C18 reads as a join runbook. Under the post-fix
tally it is a one-field read plus an arithmetic comparison.

BLOCKED today at step 2 of the runbook: `deletedAt` and `lastSeen` are
V2-only, so entity existence cannot be tested at all.

RAW glue 3, POST-FIX 1.

### C19. Exception and risk acceptance round-trip

```sh
stave list issue --limit 5000 | scripts/scrub.sh > iss.jsonl
stave list vulnerability_finding --limit 50000 | scripts/scrub.sh > vf.jsonl
# external: exceptions.jsonl from the GRC register
jq -c --slurpfile e exceptions.jsonl '($e|INDEX(.issue_ref)) as $i | . + {exc: $i[.id]}' iss.jsonl > j.jsonl
jq -c 'select(.exc != null and .status == "OPEN" and .openReason != "RISK_ACCEPTED")' j.jsonl
jq -c --slurpfile i iss.jsonl '($i|INDEX(.id)) as $x | select(.openReason == "RISK_ACCEPTED" and ($x[.id].exc == null))' j.jsonl
jq -c 'select(.exc.expires_at != null and (.exc.expires_at|fromdate) < now)' j.jsonl
jq -s 'group_by(.id) | map(select(.[0].exc) | {id:.[0].id, reopens: ...}) | .[]' j.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list issue` | n/a | yes | |
| 2 | [stave] `list vulnerability_finding` | n/a | yes | |
| 3 | shell load the GRC exception register | n/a | yes | |
| 4 | jq join issues to the exception register | match against an external set on a key | yes | P1, P3 |
| 5 | jq exceptions filed but not suppressed in the graph | presence and state mismatch, register side | yes | P1, P5 |
| 6 | jq suppressed in the graph with no filed exception | presence and state mismatch, graph side | yes | P1, P5 |
| 7 | jq exceptions past expiry | compare an external date to now | yes (the expiry lives in the register) | |
| 8 | jq count reopenings per issue with an accepted exception | count occurrences per key over time | **no** (gs23 `issueHistoryEvents`) | P2 |

BLOCKED today at every step: `openReason`, `rejectionExpiredAt`,
`reopenedAt`, and `statusChangedAt` are all unselected, and finding-level
`ignoreRules` with them, so the graph half of every comparison is empty.

RAW glue 5, POST-FIX 4.

### C20. Asset claiming and contest

```sh
stave list cloud_resource --limit 50000 | scripts/scrub.sh > res.jsonl
stave list issue --limit 5000 | scripts/scrub.sh > iss.jsonl
# external: teams.jsonl, the ownership registry
jq -c '. + {attribution: (if .tags.owner then {team:.tags.owner, basis:"tag"}
        elif (.projects|length)>0 then {team:.projects[0].name, basis:"project"}
        else {team:.subscriptionName, basis:"account-inherited"} end)}' res.jsonl > attr.jsonl
jq -c --slurpfile t teams.jsonl '($t|INDEX(.name)) as $i
   | . + {team_state: ($i[.attribution.team].status // "unknown")}' attr.jsonl > a2.jsonl
jq -c 'select(.team_state != "active")' a2.jsonl
jq -c --slurpfile r a2.jsonl '($r|INDEX(.id)) as $i | . + {attr: $i[.entitySnapshot.id].attribution}' iss.jsonl > ai.jsonl
jq -s 'group_by(.attr.team) | map({team:.[0].attr.team, issues:length})' ai.jsonl
jq -c 'select(.attribution.basis == "tag" and ((.tags.owner_updated // "1970-01-01"|fromdate) < (now - 31536000)))' a2.jsonl
```

| # | Stage | Purpose | Survives | Gap |
|---|---|---|---|---|
| 1 | [stave] `list cloud_resource` (V2) | n/a | yes | |
| 2 | [stave] `list issue` | n/a | yes | |
| 3 | shell load the ownership registry | n/a | yes | |
| 4 | jq derive the attribution and its basis in precedence order | derive a provenance label from several fields | yes | D5 |
| 5 | jq join attributions to the registry, flag dissolved teams | match against an external set and flag misses | yes | P1, P3, P5 |
| 6 | jq join issues to attributed resources | match two streams on a key | yes | P1 |
| 7 | jq group by team, count resources and issues | group and count on a derived key | yes | P2 |
| 8 | jq flag attributions resting on a stale tag | derive a staleness flag | yes | |
| 9 | **[SIMULATE]** render the reassignment that would be made | produce an artifact without performing the action | yes | |

Stage 4 is the strongest `explain` case in the catalogue. The runbook
asks for the attribution basis to be presented so a team can dispute it,
and no field carries the basis: it is the precedence rule itself that
has to be reported. That is a derived value whose derivation is the
output.

BLOCKED today at steps 1 to 3 of the runbook: `owners`, `projects`, and
`tags` are V2-only, so nothing can be attributed and there is nothing to
contest.

RAW glue 6, POST-FIX 6.

---

## The two tallies

### Glue-stage counts

| Runbook | RAW | POST-FIX | Deleted |
|---|---|---|---|
| A1 | 7 | 5 | 2 |
| A2 | 7 | 4 | 3 |
| A3 | 4 | 3 | 1 |
| A4 | 5 | 2 | 3 |
| A5 | 4 | 3 | 1 |
| B6 | 5 | 5 | 0 |
| B7 | 6 | 6 | 0 |
| B8 | 6 | 6 | 0 |
| B9 | 6 | 6 | 0 |
| B10 | 5 | 4 | 1 |
| B11 | 4 | 4 | 0 |
| B12 | 7 | 6 | 1 |
| B13 | 5 | 5 | 0 |
| C14 | 5 | 5 | 0 |
| C15 | 5 | 5 | 0 |
| C16 | 5 | 5 | 0 |
| C17 | 6 | 5 | 1 |
| C18 | 3 | 1 | 2 |
| C19 | 5 | 4 | 1 |
| C20 | 6 | 6 | 0 |
| **Total** | **106** | **90** | **16** |

**The delta is 16 of 106, or 15.1 percent.** That number understates
what it is measuring, because it is not spread evenly:

| Class | RAW | POST-FIX | Deleted | Share deleted |
|---|---|---|---|---|
| A (graph only) | 27 | 17 | 10 | 37 percent |
| B (external join) | 44 | 42 | 2 | 5 percent |
| C (spoke team) | 35 | 31 | 4 | 11 percent |

Read plainly: **more than a third of the apparent glue in the
graph-only class is our own document debt, and almost none of the glue
in the external-join class is.** The four tickets buy back most of what
class A appears to need and change class B hardly at all, because class
B's work is bridging to data no security graph holds.

### What the deleted stages were

All sixteen, so the delta can be checked rather than taken on trust:

| Runbook | Stage | Ticket that deletes it |
|---|---|---|
| A1 | group by severity and status | gs23 |
| A1 | join issues to users for a contact | qijl (`assignee` nested) |
| A2 | match a CVE id against a name string | qijl (`vulnerabilityExternalId`) |
| A2 | group by account and resource type | gs23 |
| A2 | collect owner contacts | rsh6 (`owners` on V2) |
| A3 | count by severity | gs23 |
| A4 | join audit entries to accounts | qijl (`lastLoginAt`) |
| A4 | last activity per account | qijl |
| A4 | accounts with no matching entry | qijl |
| A5 | join controls to frameworks | qijl (`SecurityFramework.controls`) |
| B10 | controls disabled at any point | gs23 (history roots) |
| B12 | open issues with no ticket | j1xi (`hasServiceTicket`) |
| C17 | keep a dated snapshot per run | gs23 (history roots) |
| C18 | join resolved issues to resources | qijl (`resolutionReason`) |
| C18 | classify remediated versus evaporated | qijl |
| C19 | count reopenings per issue | gs23 (history roots) |

Eight of the sixteen fall to `qijl`, the cheapest of the four tickets
and the one that is nothing but widening field selections on roots
already bound. Six fall to `gs23`, one each to `rsh6` and `j1xi`.

### What the delta does and does not change

It does not change which verbs qualify. Scoring under both tallies
produces the same single qualifier. What it changes is the character of
the leading prior: under the raw tally, `join` appears across all three
classes and looks like a general-purpose need. Under the post-fix tally,
every one of its thirteen appearances is a bridge to data outside Wiz.
`join` is not a stream verb this tool is missing. It is the boundary
between the security graph and everything else, and that is a different
proposition to argue for.

---

## Scoring

Applied per section 3 of the registration, unchanged. N = 3, M = 3,
glue kinds G1 to G6 as enumerated. No kind was added; one strain is
noted below. Scoring is over the **post-fix** tally, with raw figures
given where they differ.

### Counting convention, declared before the numbers

M counts, per runbook, the number of stave invocations plus named glue
stages the verb would replace. One stage is one distinct analytical
purpose. **Key normalisation is a distinct purpose from matching**, so a
`join` verb absorbs the match stage and not the normalise stage that
precedes it. This convention is load-bearing and its sensitivity is
reported after the table.

### Per-runbook, per-verb collapse counts

Blank means the verb does not appear in that runbook. Numbers are the
stages that verb would replace in that runbook, post-fix.

| | A1 | A2 | A3 | A4 | A5 | B6 | B7 | B8 | B9 | B10 | B11 | B12 | B13 | C14 | C15 | C16 | C17 | C18 | C19 | C20 | N | median M |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| P1 `join` | | 1 | 1 | | | | 3 | 4 | 3 | 2 | 1 | 1 | 3 | | 2 | 2 | | | 3 | 2 | 13 | 2 |
| P2 `roll-up` | 1 | | | 1 | 1 | 1 | 1 | 1 | 1 | 1 | | 1 | 1 | 1 | 1 | 1 | 2 | 1 | | 1 | 16 | 1 |
| P3 `coverage` | | | | | | 2 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | | 1 | 1 | | | 1 | 1 | 12 | 1 |
| P4 `diff` | | | | | | | 1 | | 1 | | | 1 | 1 | | | | 2 | | | | 5 | 1 |
| P5 `reconcile` | | | | | | | 4 | | 4 | | | 2 | 4 | | | | | | 2 | | 5 | 4 |
| D1 `sort --by` | 1 | 1 | 1 | | | 1 | | 1 | | | 1 | | | 1 | | 1 | | | | | 8 | 1 |
| D2 `dedupe` | | 1 | | | | | 1 | | | | | 1 | | | | | 1 | | | | 4 | 1 |
| D3 `pivot` | 1 | | | | 1 | 1 | 1 | | | | | | | | | | | | | | 4 | 1 |
| D4 `topn` | | | 2 | | | | | | | | 1 | | | 2 | | 1 | | | | | 4 | 1.5 |
| D5 `explain` | 1 | | | | 1 | | | | | | | | | | | | | | | 1 | 3 | 1 |
| D6 `watch` | | | | | | | | | | | | | | | | | | | | | 0 | n/a |
| D7 `diff --since` | | | | | 1 | | | | | | | | | | | | 2 | | | | 2 | 1.5 |

Raw-tally differences. P1 gains A1 (1), A4 (2), A5 (1), C18 (1) and B12
rises to 2, for N = 17 and an unchanged median of 2. P2 gains A2, A3,
and C19 at 1 each for N = 19, median unchanged. P5 gains a stage in B12
and C19 for a median of 4, unchanged. D3 gains A2 for N = 5, median
unchanged. D7 gains B10 (1) for N = 3, median 1, which moves it from
failing conjunct 1 to failing conjunct 2. Nothing crosses a threshold
into qualifying in either direction.

### Result table

| Verb | N (≥3) | median M (≥3) | Glue kinds | Qualifies |
|---|---|---|---|---|
| P1 `join` | 13 ✓ | 2 ✗ | G1, G3 | **no** |
| P2 `roll-up` | 16 ✓ | 1 ✗ | G2 | **no** |
| P3 `coverage` | 12 ✓ | 1 ✗ | G1, G5 | **no** |
| P4 `diff` | 5 ✓ | 1 ✗ | G1, G5 | **no** |
| P5 `reconcile` | 5 ✓ | 4 ✓ | G1, G3, G5 | **yes** |
| D1 `sort --by` | 8 ✓ | 1 ✗ | G6 | **no** |
| D2 `dedupe` | 4 ✓ | 1 ✗ | G2, G6 | **no** |
| D3 `pivot` | 4 ✓ | 1 ✗ | G2 | **no** |
| D4 `topn` | 4 ✓ | 1.5 ✗ | G6 | **no** |
| D5 `explain` | 3 ✓ | 1 ✗ | G5 | **no** |
| D6 `watch` | 0 ✗ | n/a | none | **no** |
| D7 `diff --since` | 2 ✗ | 1.5 | G4 | **no** |

One of twelve qualifies: `reconcile`, a prior. Zero decoys qualify.

D7 is reported separately per the registration's collision ruling. It
fails conjunct 1 post-fix at N = 2, and fails conjunct 2 raw at N = 3
with median M = 1. Its post-fix breadth is low for a specific reason
worth separating from its merit: `gs23` binds the history and trend
roots, which absorbs the two-point comparison in B10 outright, and C18's
metric recomputation turned out to be a partition comparison rather than
a temporal one. P4 also fails, on compression. Neither result supports
the other and D7's numbers are not pooled into any decoy summary.

### The strain in conjunct 3, reported not resolved

Per-field disagreement across matched records (the core of P4 and part
of P5) sits awkwardly in the enumerated kinds. G1 covers the matching.
The comparison itself was scored as G5 derivation, reading G5 literally
as computing a field from other fields across a stream, where the stream
is the joined one. No kind was added. If the gate judges that per-field
disagreement is a kind of its own, P4 and P5 are the two entries
affected and P5 qualifies either way on N and M.

### The pattern in the failures, which is a result about the predicate

Eleven of twelve entries fail on conjunct 2, and they fail the same way.
A verb that does one thing replaces one stage per runbook, so its median
M is 1 no matter how many runbooks it appears in. `roll-up` appears in
sixteen of twenty runbooks and still fails. `sort --by` appears in
eight and fails. **Under this predicate only a composite verb can clear
M = 3**, because only a composite absorbs several stages in a single
runbook. `reconcile` qualifies precisely because it is registered as
three operations in one verb.

That is not evidence that composites are better. It is a property of the
predicate, visible now that it has been run, and the honest reading is
that conjunct 2 is measuring composition rather than compression. Under
the registration's own terms this belongs at the gate, not in an
adjustment here.

### Sensitivity, since M = 3 is close for one entry

`join` has median M = 2 with thirteen appearances. Under a different but
defensible convention, where a `join --on <expr>` verb takes a key
expression and therefore absorbs the key normalisation stage that
precedes each match, its per-runbook counts rise by one in the ten
runbooks that carry a normalisation stage, the median moves to 3, and it
qualifies.

That convention was not adopted, because adopting it after seeing that
it promotes the registered leading candidate is the exact tuning the
registration exists to prevent. It is reported so that the gate can see
that the leading prior's failure rests on a stage-decomposition choice
made before scoring and not on a wide margin.

No other entry is within one of a threshold.

---

## Honest limits

**The arm measures an LLM, not an operator.** These pipelines were
written by the same kind of agent that would execute them, so the tally
records what an LLM reaches for. The reach here is `jq`, because `jq` is
the tool this agent writes fluently. An operator with the same data in a
warehouse would push every join and every group into SQL, where both are
free, and would propose none of these verbs. That is not a small caveat:
five of the twelve entries exist in the registration only because the
composition substrate assumed is a shell pipeline.

**The catalogue's phrasing pushed several of these shapes.** Four
specific instances, all of which inflate or deflate a score for reasons
unrelated to the tool:

1. **`reconcile` is in three runbook titles.** B7 "CMDB three-bucket
   reconciliation", B9 "Control assertion reconciliation", B10 "Change
   drift reconciliation", and B12's objective is reconciliation in all
   but name. The single verb that qualified shares its name with the
   word the catalogue uses for the runbooks it qualified on. That is the
   most serious confound in this document and it cannot be separated
   from the result by any amount of care in the scoring.
2. **B6's success criterion imposes `coverage` on the whole of class
   B.** "Every later cross-check in this class states its coverage
   ceiling up front" means a faithful pipeline for B7 through B13 opens
   with a correspondence measurement whether or not the runbook itself
   asks for one. P3's N of 12 is substantially a consequence of that one
   sentence.
3. **"Rank the survivors" and "rank causes" seed `sort` and `topn`
   directly.** A3 and C14 use the word, and both decoys appear there.
4. **Every runbook ends in a dated artifact.** "It is Tuesday, the
   review is Thursday", "a dated table", "a dated artifact stating
   as-of time". Nothing in the catalogue asks for a live view, which is
   why `watch` scored zero. That zero is a fact about the catalogue's
   framing and not about whether operators want a live view.

**M is sensitive to stage decomposition.** Stated above with the one
entry it changes. A coarser decomposition would suppress every M and a
finer one would inflate them uniformly; the convention was fixed before
counting and applied identically to priors and decoys.

**Gap marks for decoys were made deliberately and are still probably
undercounted.** Writing naturally produces `jq 'sort_by(...)'` without
noticing that a verb was wanted. Each of the twenty runbooks was walked
against all twelve entries after the pipeline was drafted, specifically
to catch that, and D1 gained five marks and D2 gained three in that
pass. The direction of any residual error is toward undercounting
decoys, which biases toward the priors.

**One input was overread.** The brief scoped this arm to sections 1
through 3 of `verb-candidate-registration.md`. The file was read in one
call, so sections 4 and 5 were also read. Neither describes the control
arm; section 4 asks for a server-analogue column recorded alongside the
score and section 5 describes the gate's reporting shape. No
server-analogue column is presented here, since that was not this arm's
assignment, and the server-analogue reasoning that does appear
(`issuesGroupedByValue`, the trend and history roots,
`securityFrameworksDiff`, `hasServiceTicket`) comes from
`field-surface-audit.md`, which is a permitted input and is the required
source for the survives-the-fix tagging. The sealed baseline and the
gate-scoped amendments were not read, and neither was
`catalogue-provenance.md`.

**Nothing was executed.** Every pipeline above is unrun. The `jq` in
them is illustrative and several stages elide the exact expression, since
the purpose line is what is tallied and the expression is not. Anyone
turning one of these into a real run must take it through the safety
coach per invocation and through `scripts/scrub.sh` per read, and should
expect the blocked steps to fail for the reasons named rather than to
work.
