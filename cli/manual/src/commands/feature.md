# bdd feature

Feature discovery and creation. Feature files are the BDD altitude of
the workflow: each requirement gets a scenario in one, tagged with its
`@REQ-...` id.

```text
Usage: bdd feature [OPTIONS] <COMMAND>

Commands: list, show, create
```

---

## bdd feature list

List every feature file under the root with its feature name and
scenario count.

```bash
bdd feature list
```

```json
[
  {
    "path": "features/string_calculator.feature",
    "name": "String Calculator",
    "scenarios": 3
  }
]
```

---

## bdd feature show

Show one parsed feature file — its name, scenarios, tags, and steps —
as structured JSON rather than raw text.

```text
Usage: bdd feature show [OPTIONS] <PATH>
```

The path is relative to `--root`:

```bash
bdd feature show features/string_calculator.feature
```

```json
{
  "path": "features/string_calculator.feature",
  "name": "String Calculator",
  "scenarios": [
    {
      "name": "Empty string returns zero",
      "tags": ["@REQ-001"],
      "steps": [
        "Given the input \"\"",
        "When add is called",
        "Then the result is 0"
      ]
    }
  ]
}
```

A file that is not valid Gherkin fails with the parser's diagnosis.

---

## bdd feature create

Create a feature file. The file is **staged**, not written to the
working tree — review with [`bdd changes show`](changes.md) and apply
with `bdd changes commit`.

```text
Usage: bdd feature create [OPTIONS] --path <PATH> --name <NAME>
```

| Flag | Description |
| --- | --- |
| `--path <PATH>` | Feature file path relative to `--root` (conventionally under `features/`). |
| `--name <NAME>` | Feature name — the text after `Feature:`. |

```bash
bdd feature create --path features/string_calculator.feature --name "String Calculator"
bdd changes show
bdd changes commit
```

The staged file contains the `Feature:` header ready for scenarios:

```gherkin
Feature: String Calculator
```

Add scenarios with [`bdd scenario add`](scenario.md) — don't edit the
staged file by hand.

## See also

- [`bdd scenario`](scenario.md) — populate features with tagged scenarios.
- [`bdd validate`](validate.md) — parse-check all features, staged included.
