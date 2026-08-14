# The interactive shell

Running `bdd` with no subcommand in a terminal opens a REPL. It prints
the help once, shows the banner and the session's model status, and
then reads commands until you leave.

```text
bdd> spec list
[
  { "id": "REQ-001", "title": "Empty string returns zero", "status": "implemented" }
]
bdd> test --feature features/calculator.feature
...
bdd> exit
```

## Behavior

- **No prefix needed.** Type `spec list`, not `bdd spec list`. A
  leading `bdd` is forgiven if you type it anyway.
- **Inherited flags.** Commands inherit the shell's `--root` and
  `--model` unless the line supplies its own:

```bash
bdd --root ~/code/calculator --model qwen3:30b
# every command in this shell now targets that root and model
```

- **Quoting works.** Lines are tokenized with shell rules, so
  `scenario add --step "Given a calculator"` behaves as expected.
  Unbalanced quotes report `unreadable input` and the shell continues.
- **Errors don't kill the shell.** A failing command prints its error
  and returns to the prompt.
- **Blank lines** are ignored.

## Leaving

- `exit` or `quit`
- <kbd>Ctrl</kbd>+<kbd>C</kbd> (interrupt)
- <kbd>Ctrl</kbd>+<kbd>D</kbd> (end of input)

On exit the shell prints a summary of how many commands ran.

## Session history

Line history is kept across sessions in `.bdd-history` under the
project root. Use <kbd>↑</kbd>/<kbd>↓</kbd> to recall previous
commands and <kbd>Ctrl</kbd>+<kbd>R</kbd> for reverse search. If the
history cannot be saved, the shell says so and exits normally.

## Model announcement at startup

The first prompt is preceded by one line describing the session's
model:

| Situation | Announcement |
| --- | --- |
| Configured in `.bdd-mcp.toml` | `Model: qwen3:30b (from configuration).` |
| No config, models installed | `Model set for this session: qwen3:8b (not saved - keep it with: bdd model use qwen3:8b).` |
| Ollama up, no models | `No models are installed in Ollama - LLM-backed generation will fall back to templates.` |
| Ollama unreachable | `Ollama is not reachable - LLM-backed generation will fall back to templates.` |

## The greenfield nudge

When all three signs of a brand-new project line up —

1. this is the first shell session in the root (no `.bdd-history` yet),
2. a model is ready, and
3. there is no `requirements/requirements.json`

— the shell offers to start the loop before the first prompt:

```text
It appears you are in a greenfield - this project has no requirements/requirements.json yet.
Start with the greenfield command now? [y/N]
```

`y` runs [`bdd greenfield`](commands/greenfield.md) immediately;
anything else declines and the shell carries on:

```text
No problem - type greenfield any time, or spec draft to begin with the spec.
```

## When there is no terminal

If stdin is not a terminal (piped input, CI), bare `bdd` prints the
help and exits instead of opening the shell.
