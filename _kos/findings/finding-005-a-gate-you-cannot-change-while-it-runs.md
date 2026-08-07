# finding-005: a gate you cannot change while it is running

2026-08-07, during the first live validation run. Not fixed, because the
fix is not ours to make; bd `aae-orc-9pe1` is where it lands.

## What happened

The safety coach is a Claude subagent defined by
`.claude/agents/stave-safety-coach.md`. Its check 4 halted a bounded
`list` three times in one session for being a repeat, a rule written for
full-connection walks. The operator directed a change to the rule rather
than an exception to it, and the change was committed (`580dd8c`).

Re-proposing the same read to a freshly spawned coach produced a fourth
HALT. Its reason:

> the check-4 text available to me in this session is unchanged and
> still names "a repeat of a bulk pull already performed this session"
> as an unconditional HALT trigger

Asked to read its own definition from disk and compare, it confirmed:

> The file on disk is a materially revised version of check 4. My
> in-context instructions carry an older, simpler check 4. That dated
> correction postdates whatever version got baked into my context.

**An edit to an agent definition does not reach a subagent spawned after
the edit.** Spawning is not the reload boundary.

## Where the boundary actually is

Not session start, which was the first guess and was wrong. Two edits
to the same file, same session, different fates:

| Commit | Change | In the coach's context? |
|---|---|---|
| `ebc48a2` | added the `security_framework` fan-out HALT | **yes**, quoted back verbatim |
| `580dd8c` | rewrote check 4 | no |

Context compaction happened between them. That is the only session
event that fits, so the working hypothesis is that the definition is
snapshotted at session start and again at compaction. Stated as a
hypothesis because it rests on one observation with n=2, and the harness
is not ours to read.

## The direction matters more than the fact

I got this wrong in the alarming direction first, and the correction is
the useful part of the finding.

On the first diagnostic the coach said the two-kinds carve-out "does not
exist in what I was given," and I read that as the `security_framework`
rule being absent. That would have meant a hardening shipped in the
morning had protected nothing all day. Asked the narrow question
directly, the coach corrected itself and quoted the rule back from its
own instructions. Only the restructuring was new.

So the staleness here ran in the safe direction: **the stale gate was the
stricter gate.** Nothing was under-protected. The cost was an authorised
loosening that could not take effect, which is an inconvenience.

That will not always be true, and the general statement is the one to
carry:

> A control that is snapshotted cannot be hardened in response to
> something you learn while it is running. You can only harden it for
> next time.

For a gate in front of a production tenant, the case that matters is the
one where a session discovers a new hazard, writes the rule, and keeps
going believing it is covered. The rule is real, the commit is real, the
tests pass, and the gate never sees it.

## Two lessons about verification, not about the gate

**A subagent's report on its own instructions is a claim, not a
reading.** The first answer was confidently wrong on a point of fact
about its own context. It became reliable only when asked a narrow
yes-or-no question with the quote demanded, and when told plainly that
the unwelcome answer was the more useful one. Broad questions about
one's own state invite reconstruction; narrow ones invite lookup.

**The gate refusing my claim was correct and is worth defending.** The
coach halted because it could not verify that the rule had changed, and
said it would not let a justification embedded in the caller's own
framing resolve a rule question. That is the property that makes the
gate a gate. The right response was to stop, not to argue, and the
diagnostic that followed proved the coach right and me wrong.

## What follows

`aae-orc-9pe1` already asks whether the gate can move into the driving
agent's loop, as a Bash-tool hook or an MCP-hosted tool. This finding
adds a requirement that neither shape had before: **whatever holds the
gate must read current state rather than a snapshot**, and must be able
to say which version it applied. A gate that cannot report its own
version cannot be audited after the fact, and this session would have
had no way to discover the skew without asking the reviewer to read its
own file.

Cheap interim, available today: when a session changes the coach and the
work continues, either restart before the next tenant read, or record in
the runlog that the gate in force is the pre-change one. The second is
worse and is still better than assuming.

## Cross-references

- `.claude/agents/stave-safety-coach.md` check 4; `580dd8c`, `ebc48a2`
- `.claude/rules/safety-coach-gate.md` — "a control that depends on
  someone remembering fails exactly when the pace picks up." A control
  that depends on a snapshot fails the same way, one level down.
- `docs/design/runlog-harness.md` § "The gate and its limits" — the
  harness already declined to claim it could prove a subagent was
  consulted. It also cannot prove which rules that subagent held.
- finding-004 — the sibling. There a control's exemption keyed on
  something its reviewer could not see; here a control's TEXT is
  something its own author cannot see.
