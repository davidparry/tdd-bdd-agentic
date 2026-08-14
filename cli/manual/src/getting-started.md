# Getting started

## Install

Download an installer from the
[latest release](https://github.com/davidparry/tdd-bdd-agentic/releases/latest)
(macOS Apple Silicon and Intel, Linux x86_64 and arm64, Windows
x86_64), or build from source:

```bash
cd cli
cargo build --release
./target/release/bdd --help
```

The shell installer places the binary in `$CARGO_HOME/bin` (usually
`~/.cargo/bin/bdd`) and writes an install receipt to
`~/.config/bdd-cli/bdd-cli-receipt.json`.

## Your first session

Run bare `bdd` in a terminal. You get the help, the banner with the
version, the model status, and an interactive prompt:

```text
$ bdd

  ╭──────────────────────────────────╮
  │                                  ▼
  │    > bdd  v0.2.4                 │
  │    spec → RED → GREEN → REFACTOR │
  ▲                                  │
  ╰──────────────────────────────────╯

Model set for this session: qwen3:8b (not saved - keep it with: bdd model use qwen3:8b).
Interactive shell - type commands without the bdd prefix (e.g. spec list).
bdd>
```

## Two ways to begin a project

**Guided, from zero** — one command runs the whole loop with exactly
two human gates (approving the spec wording and approving generated
tests):

```bash
mkdir calculator && cd calculator
bdd greenfield
```

**Step by step** — scaffold, then drive each phase yourself:

```bash
bdd init --language rust --name "String Calculator"
bdd spec draft          # describe what to build in plain words; with a
                        # model resolved it proposes title, story, and
                        # criteria for you to edit (manual prompts otherwise)
bdd spec validate       # structure gate
bdd spec refine REQ-001 # wording gate
bdd changes commit      # apply the staged spec
bdd test                # expect RED
# ...implement...
bdd test                # expect GREEN
bdd refactor --note "extract parser"
bdd test                # still GREEN
bdd status              # confirm REQ-001 is ready to mark
bdd spec mark-implemented REQ-001 && bdd changes commit
```

## Working against an existing project

Every command takes `--root` (see [Global flags](global-flags.md)), so
you can point the CLI at any project:

```bash
bdd --root ~/code/my-kata inspect
bdd --root ~/code/my-kata spec validate
```
