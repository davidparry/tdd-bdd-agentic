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
bdd --model qwen3-coder-next:latest steps generate
bdd --model llama3:8b greenfield   # another model; mileage will vary
```

Model resolution order (see [bdd model](commands/model.md) for the
full story):

1. `--model` flag — this invocation only.
2. `model` in `.bdd-mcp.toml` — the persisted project choice.
3. Discovery — the first installed Ollama model, session-only.

## `--debug`

Verbose diagnostic logging: the full prompts sent to the model and its
responses, cache hits and misses, resolved configuration, MCP tool
calls, and test-runner activity. Without the flag only high-level
lifecycle events (info and above) are logged.

Diagnostics are written to daily-rolling files under `.bdd-log/` in
the project root (gitignored; safe to delete at any time) — stdout and
stderr are never touched, so JSON output, the MCP stdio protocol, and
user-facing messages stay clean either way. Log writes go through an
in-memory queue drained by a worker thread, so logging never blocks
the command. If the log directory cannot be created, diagnostics fall
back to stderr.

```bash
bdd --debug implement REQ-003             # full prompts and replies in the log
tail -f .bdd-log/bdd.log.$(date +%F)      # watch the diagnostics live
```

The `RUST_LOG` environment variable overrides both the default and
`--debug` with per-module directives, e.g.
`RUST_LOG=bdd_cli::adapters=trace bdd test`.

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
