# Run-harness worked example

Synthetic. Nothing here contacts a tenant.

| File | What it is |
|---|---|
| `stub-stave` | Stands in for the `stave` binary. Answers a few invocations with synthetic records and writes synthetic audit lines in the real schema, so the runlog-to-audit join has something to join on. |
| `walkthrough.sh` | Drives `scripts/runlog.sh` end to end against the stub and prints the runlog. `--write` refreshes `example-runlog.jsonl`. |
| `example-runlog.jsonl` | The committed output, so the mining stage can see the shape before any real run exists. |

```sh
examples/runlog/walkthrough.sh            # run, print, discard
examples/runlog/walkthrough.sh --write    # also refresh example-runlog.jsonl
```

The coach blocks in `walkthrough.sh` are **fixtures**. In a real run each
one is the verbatim output of the `stave-safety-coach` subagent,
consulted before the invocation it names.

The stub's records deliberately carry a person's name, an ARN, an email,
and a resource name, so the example shows the scrubber working rather
than merely running. Compare `data/issues.jsonl` in the run directory
against the stub's source to see which fields survived.

Two absolute paths in the `run_start` entry of `example-runlog.jsonl`
are replaced with `<run-dir>`, since the run happens in a temp
directory. Everything else is verbatim.

Design and limits: `docs/design/runlog-harness.md`.
