# bdd refactor

Begin a refactor step. Only allowed on GREEN — the discipline's core
rule is that you never restructure code while tests are failing.

```text
Usage: bdd refactor [OPTIONS]
```

MCP tool equivalent: `start_refactor`.

## Flags

| Flag | Description |
| --- | --- |
| `--note <NOTE>` | What you intend to refactor and why. Recorded in the refactor log. |
| `--root <ROOT>` | Project root. Defaults to `.`. |
| `--model <MODEL>` | Accepted (global flag) but unused. |

## Examples

On GREEN:

```bash
bdd refactor --note "extract the delimiter parser from add()"
```

```json
{
  "phase": "REFACTOR",
  "nextStep": "Refactor with the tests as your safety net, then 'bdd test'. Passing returns you to GREEN; a failure means the refactor broke behavior."
}
```

Attempting it on RED is refused with exit status 1:

```text
Error: refactoring is only allowed on GREEN - you are RED. Make the tests pass first.
```

## The refactor loop

```bash
bdd test                                  # GREEN - safe to restructure
bdd refactor --note "collapse duplicate parsing"
# ...restructure, behavior unchanged...
bdd test                                  # GREEN again: refactor complete
```

If that final `bdd test` fails, the phase drops to RED: the refactor
changed behavior, and the failing tests tell you exactly where.

## Why the note matters

Each `--note` is appended to the `refactorLog` that
[`bdd state`](state.md) reports. Over a kata or a workshop, the log
becomes the narrative of deliberate design decisions — which is the
half of TDD that "make it pass" alone never captures.

## See also

- [`bdd state`](state.md) — the phase and the accumulated log.
- [The workflow](../workflow.md) — where REFACTOR sits in the machine.
