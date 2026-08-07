# Safety Coach Gate: Consult Before Every stave Invocation

Behavior-trigger rule. Same shape as the global force-push abort signal
and `tenant-data-hygiene.md`. Names the keystroke, because the tenant is
a production security environment and the executor is an agent reading
runbook prose written in the imperative.

## The trigger

You are about to type `stave` followed by anything. **STOP.**

Hand the exact proposed invocation to the `stave-safety-coach` subagent
and wait for a verdict. Two outcomes:

- **CLEAR**: run it.
- **HALT**: do not run it. Stop the run. Bring the doubt to the human.

Do not run the command and then review it. Do not decide for yourself
that this one is obviously fine. The fast path for provably-offline
commands lives inside the coach, not in your judgement.

## The runbook trap this exists for

The runbook exercise (bd `aae-orc-e4jo`) is a **simulation against a
live production tenant**. Runbooks legitimately contain steps phrased as
actions: assign the owner, clear the finding, comment on the ticket,
classify the resource, accept the risk.

Every one of those is to be **simulated and never performed**. "Assign
the issue" means produce what the assignment would be. It never means
call something that assigns it.

The failure mode is not malice or carelessness. It is an agent following
imperative prose in good faith, one step at a time, each step locally
reasonable. That is precisely the failure a per-command external check
catches and a good intention does not.

## What HALT means operationally

1. Stop the run. Not just the step. A halt means the sequence pauses.
2. Record it in the run log as a `halt` entry with the coach's stated
   doubt, verbatim.
3. Bring it to the human with the doubt and the coach's TO RESOLVE line.
4. The human chooses: skip the step, proceed with a modification, or
   stop the run.
5. Never make that choice yourself, and never infer it from context.

A halt is cheap. The human resolves it in seconds. A false CLEAR against
a production security tenant is not recoverable in the same way.

## Do not route around it

Specifically, these are all the same violation:

- Running the command first and asking afterwards
- Batching many commands into one review to save turns
- Skipping review for a command you ran successfully earlier, since the
  argument that matters may have changed
- Deciding a command is on the fast path yourself
- Treating the SDK write guard as sufficient, since it has never been
  commissioned against a live mutation and defence in depth means two
  independent checks
- Wrapping stave in a script and reviewing the script once instead of
  the invocations it makes

The last one is the subtle one. If a harness generates invocations, the
gate belongs at the point of generation, inside the harness, not at the
point the harness was written.

## In the run harness

bd `aae-orc-e4jo.9` builds the runbook harness. The gate is structural
there, not remembered: the harness routes every invocation through the
coach before executing it, records the verdict alongside the audit
trail, and halts the run on the first HALT.

The same reasoning as scrubbing by construction. A control that depends
on someone remembering fails exactly when the pace picks up, which is
when it is needed.

## Why the coach has no execution tools

`stave-safety-coach` is granted Read, Grep, and Glob and nothing else.
The reviewer must not be able to perform the act it reviews. If a future
change gives that agent Bash, the gate has quietly become advisory.

## Cross-references

- Agent: `.claude/agents/stave-safety-coach.md`
- `tenant-data-hygiene.md` guards what leaves the tenant; this guards
  what reaches it. Both fire before the same keystroke.
- `docs/design/read-only-posture-and-permissions-report.md` D1 (the
  write guard is unconditional) and its own admission that the guard is
  untested against a live mutation, which is why it is not sufficient
  alone
- Behavior-trigger siblings: orc `tooling-friction.md`, `agent-tools.md`,
  `upstream-claim-gate.md`
