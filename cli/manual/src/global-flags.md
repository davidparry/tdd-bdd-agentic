# Global flags

These flags are accepted by `bdd` itself and by every command. Place
them anywhere on the command line.

## `--root <ROOT>`

The project root — the directory where `requirements/requirements.json`
and `.bdd-mcp.toml` live, and the base for every relative path the CLI
reads or writes. Defaults to the current directory.

```bash
bdd --root ~/code/calculator spec list
bdd spec list --root ~/code/calculator   # same thing
```

In the [interactive shell](interactive-shell.md), commands inherit the
shell's `--root` unless a line supplies its own.

## `--model <MODEL>`

Override the LLM model for this invocation only. This wins over the
configured model and over discovery, and is never written to
configuration:

```bash
bdd --model qwen3:30b steps generate
bdd --model llama3:8b greenfield
```

Model resolution order (see [bdd model](commands/model.md) for the
full story):

1. `--model` flag — this invocation only.
2. `model` in `.bdd-mcp.toml` — the persisted project choice.
3. Discovery — the first installed Ollama model, session-only.

## `-V`, `--version`

Prints the version compiled into the binary (from `Cargo.toml` at
build time).

## `-h`, `--help`

Every command and subcommand answers `--help` with its synopsis,
arguments, and flags.

## Exit status

- `0` — success (including replies whose JSON reports e.g. an invalid
  spec: the *command* succeeded).
- `1` — the command itself failed or refused: unknown requirement id,
  missing runtime (`runtime_missing`), unreachable Ollama for a
  model-required operation, unparseable arguments, and similar.
