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

## Why serve tools instead of letting the agent edit files?

- **No escape hatches.** The agent gets exactly these tools — no
  shell, no arbitrary file writes. Mutations go through the
  [staging area](../staged-changes.md) for human review.
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
