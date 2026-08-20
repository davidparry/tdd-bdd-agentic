# bdd init

Scaffold build files, a Cucumber runner, an empty requirements spec,
and the CLI's configuration in the project root. Existing files are
never overwritten — they are reported as skipped.

```text
Usage: bdd init [OPTIONS]
```

## Flags

| Flag | Description |
| --- | --- |
| `--language <LANGUAGE>` | Target language: `java`, `javascript`, `typescript`, `dotnet`, or `rust`. Prompted interactively when omitted. |
| `--name <NAME>` | Project name used inside the generated build files. Defaults to the root directory's name. |
| `--root <ROOT>` | Project root to scaffold into. Defaults to `.`. |
| `--model <MODEL>` | Accepted (global flag) but unused — `init` is fully deterministic. |

## What gets created

Every language gets the two spec-driven anchors:

- `requirements/requirements.json` — an empty, valid spec. It is also
  the root of the [spec catalog](../spec-format.md): split the backlog
  into included files later with
  [`bdd spec include add`](spec.md#bdd-spec-include).
- `.bdd-mcp.toml` — the CLI/MCP configuration.

Plus the language's build and BDD harness:

| Language | Scaffolded files |
| --- | --- |
| `java` | `pom.xml` (Maven + Cucumber-JVM), `src/test/java/RunCucumberTest.java`, `features/.gitkeep` |
| `javascript` | `package.json` (Cucumber-JS), `cucumber.js`, `features/step_definitions/.gitkeep` |
| `typescript` | `package.json`, `tsconfig.json`, `cucumber.js` (ts-node hooked in), `features/step_definitions/.gitkeep` |
| `dotnet` | `<Name>.Tests.csproj` (Reqnroll), `features/.gitkeep` |
| `rust` | `Cargo.toml` (cucumber-rs dev-dependency), `src/lib.rs`, a Cucumber test harness, `features/.gitkeep` |

## Examples

Scaffold a Rust kata in a fresh directory:

```bash
mkdir calculator && cd calculator
bdd init --language rust --name "String Calculator"
```

```json
{
  "language": "rust",
  "framework": "cucumber-rs",
  "created": [
    "requirements/requirements.json",
    ".bdd-mcp.toml",
    "Cargo.toml",
    "src/lib.rs",
    "tests/cucumber.rs",
    "features/.gitkeep"
  ],
  "skipped": [],
  "nextStep": "Draft your first requirement with 'bdd spec draft', then 'bdd spec validate'."
}
```

Re-running is safe — everything that already exists is skipped:

```bash
bdd init --language rust
```

```json
{
  "language": "rust",
  "framework": "cucumber-rs",
  "created": [],
  "skipped": [
    "requirements/requirements.json",
    ".bdd-mcp.toml",
    "Cargo.toml",
    "src/lib.rs",
    "tests/cucumber.rs",
    "features/.gitkeep"
  ],
  "nextStep": "Draft your first requirement with 'bdd spec draft', then 'bdd spec validate'."
}
```

Omit `--language` and the CLI prompts with the supported list; an
unrecognized answer re-prompts.

## Notes

- `init` writes directly to the working tree (there is nothing to
  protect in an empty project); everything after `init` goes through
  [staged changes](../staged-changes.md).
- `init` does not install runtimes. Run
  [`bdd inspect`](inspect.md) to see whether the language's runtime is
  present before expecting `bdd test` to execute.

## See also

- [`bdd greenfield`](greenfield.md) — runs `init` as its first move.
- [`bdd spec draft`](spec.md#bdd-spec-draft) — the natural next step.
