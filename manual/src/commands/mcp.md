# bdd mcp

The embedded MCP server. This is the same workflow the CLI offers a
human, exposed to AI agents as typed tools over the Model Context
Protocol.

```text
Usage: bdd mcp [OPTIONS] <COMMAND>

Commands: serve
```

---

## bdd mcp serve

Serve the MCP tools over stdio. The process reads JSON-RPC on stdin
and writes replies on stdout, so an MCP client (Cursor, Claude
Desktop, any MCP-capable agent) launches it as a child process — you
normally never run it by hand.

```bash
bdd mcp serve --root /path/to/project
```

Client configuration (Cursor's `mcp.json` shown; others are
equivalent):

```json
{
  "mcpServers": {
    "bdd-workflow": {
      "command": "bdd",
      "args": ["mcp", "serve", "--root", "/path/to/project"]
    }
  }
}
```

## The tools served

The tool names and reply shapes are byte-compatible with the
workshop's Java `tdd-workflow-server`, so existing clients work
unchanged:

| MCP tool | CLI equivalent |
| --- | --- |
| `list_requirements` | [`bdd spec list`](spec.md#bdd-spec-list) |
| `get_requirement` | [`bdd spec show`](spec.md#bdd-spec-show) |
| `validate_spec` | [`bdd spec validate`](spec.md#bdd-spec-validate) |
| `refine_requirement` | [`bdd spec refine`](spec.md#bdd-spec-refine) |
| `requirement_mark_implemented` | [`bdd spec mark-implemented`](spec.md#bdd-spec-mark-implemented) |
| `step_definitions_find` | [`bdd steps missing`](steps.md#bdd-steps-missing) |
| `step_definition_create` | [`bdd steps generate`](steps.md#bdd-steps-generate) |
| `unit_test_create` | [`bdd unittest generate`](unittest.md) |
| `run_tests` | [`bdd test`](test.md) |
| `get_tdd_state` | [`bdd state`](state.md) |
| `start_refactor` | [`bdd refactor`](refactor.md) |
| `command_run` | — (MCP only, see below) |

## command_run: the guarded command line

`command_run` lets an agent run one dev-tool command during the
implementation phase — building, compiling, or installing what the
failing tests need. It is not a shell. Every call passes these
guardrails, checked before anything spawns:

- **Allowlist.** The program must be one of `cargo`, `mvn`, `npm`,
  `npx`, `node`, `dotnet`, `java`, `javac`, `tsc`, given as a bare
  name (never a path). `rm`, `sudo`, `sh`, `curl`, `git`, and
  everything else is refused.
- **No shell.** The command executes directly as argv, so `;`, `&&`,
  `|`, globs, and redirection are inert text — chaining a destructive
  command onto an allowed one is unexpressible.
- **Eval escapes refused.** Flags that turn an allowed tool into
  arbitrary code execution (`node -e/--eval/-p/--print`,
  `npx -c/--call`, `npm exec`/`npm x`, Maven `exec:*` goals) are
  refused.
- **Root jail.** The process runs with `--root` as its working
  directory, and no argument may be an absolute path or contain
  `..` — the command cannot name anything outside the root.
- **RED bar only.** Commands run only during the implementation
  phase. Off a RED bar the tool refuses and points at `run_tests`.
- **Timeout and output cap.** A hard timeout (default and maximum
  300 seconds) kills a hung process; each output stream is truncated
  to its last 200 lines.

This is policy-level guardrailing, not an OS sandbox: an allowed
build tool can still run build scripts. What the policy makes
unexpressible is running destructive binaries and reaching outside
the project root.

## Why serve tools instead of letting the agent edit files?

- **No escape hatches.** The agent gets exactly these tools — no
  open-ended shell, no arbitrary file writes. The one command tool is
  allowlisted, jailed to the root, and phase-gated; mutations go
  through the [staging area](../staged-changes.md) for human review.
- **The discipline is in the server.** An agent cannot skip RED,
  refactor while failing, or invent requirements: the tools refuse,
  with a `nextStep` that teaches the correct move.
- **State survives.** The phase machine lives on disk, so a
  reconnecting agent (or a human taking over in the CLI) continues
  from the same place.

## Flags

| Flag | Description |
| --- | --- |
| `--root <ROOT>` | Project root the served tools operate on. Defaults to the process's working directory. |
| `--model <MODEL>` | Model override for the serving session's generation tools. |

## Notes

- The server logs nothing to stdout except protocol traffic (stdout
  is the wire). Diagnostics go to stderr.
- One server serves one project root. Point different projects at
  different server entries.
- Generation tools use the same local Ollama resolution as the rest
  of the CLI. This CLI is developed and run against
  `qwen3-coder-next:latest`; your mileage will vary with other
  models, especially those not trained for development work. See
  [`bdd model`](model.md).
