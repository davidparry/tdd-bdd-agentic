# bdd scenario

Scenario mutations. Every scenario is tied to a requirement by a
`@REQ-...` tag, keeping the feature files traceable back to the spec.
All three subcommands write to the [staging area](../staged-changes.md).

```text
Usage: bdd scenario [OPTIONS] <COMMAND>

Commands: add, update, delete
```

---

## bdd scenario add

Append a tagged scenario to an existing feature file.

```text
Usage: bdd scenario add [OPTIONS] --feature <FEATURE> --req <REQ> --name <NAME>
```

| Flag | Description |
| --- | --- |
| `--feature <FEATURE>` | Feature file path relative to `--root`. |
| `--req <REQ>` | Requirement id the scenario implements; becomes the `@REQ-...` tag. |
| `--name <NAME>` | Scenario name. |
| `--step <STEPS>` | One full Gherkin step per flag, repeatable, in order. |

```bash
bdd scenario add \
  --feature features/string_calculator.feature \
  --req REQ-003 \
  --name "Two numbers separated by a comma are summed" \
  --step 'Given the input "1,2"' \
  --step 'When add is called' \
  --step 'Then the result is 3'
```

The staged result appended to the feature:

```gherkin
  @REQ-003
  Scenario: Two numbers separated by a comma are summed
    Given the input "1,2"
    When add is called
    Then the result is 3
```

Each `--step` must start with a Gherkin keyword (`Given`, `When`,
`Then`, `And`, `But`); the mutation is validated as real Gherkin
before it stages.

---

## bdd scenario update

Replace a scenario's steps and/or its requirement tag. The scenario is
found by feature path + scenario name; omitted parts are kept.

```text
Usage: bdd scenario update [OPTIONS] --feature <FEATURE> --name <NAME>
```

| Flag | Description |
| --- | --- |
| `--feature <FEATURE>` | Feature file path relative to `--root`. |
| `--name <NAME>` | Name of the scenario to update. |
| `--req <REQ>` | New requirement id for the tag; omit to keep the current tag. |
| `--step <STEPS>` | New steps (repeatable, full replacement); omit to keep the current steps. |

Retag a scenario without touching its steps:

```bash
bdd scenario update \
  --feature features/string_calculator.feature \
  --name "Two numbers separated by a comma are summed" \
  --req REQ-007
```

Rewrite the steps:

```bash
bdd scenario update \
  --feature features/string_calculator.feature \
  --name "Two numbers separated by a comma are summed" \
  --step 'Given the input "10,20"' \
  --step 'When add is called' \
  --step 'Then the result is 30'
```

---

## bdd scenario delete

Remove a scenario from a feature file.

```text
Usage: bdd scenario delete [OPTIONS] --feature <FEATURE> --name <NAME>
```

```bash
bdd scenario delete \
  --feature features/string_calculator.feature \
  --name "Two numbers separated by a comma are summed"
```

Deleting a scenario that does not exist fails with exit status 1 and
names the feature searched.

## The full rhythm

```bash
bdd scenario add --feature features/calc.feature --req REQ-002 --name "..." --step '...'
bdd changes show      # review the staged modify
bdd changes commit    # apply
bdd steps missing     # any steps without definitions?
bdd test              # expect RED
```

## See also

- [`bdd steps`](steps.md) — find and generate the step definitions
  behind these scenarios.
- [`bdd changes`](changes.md) — review, apply, or discard the staged mutation.
