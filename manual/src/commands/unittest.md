# bdd unittest

Unit-test generation — the TDD altitude beneath the Gherkin scenarios.
Where a scenario proves the behavior end to end, the unit test pins
down the fine-grained contract of the production code.

```text
Usage: bdd unittest [OPTIONS] <COMMAND>

Commands: generate
```

MCP tool equivalent: `unit_test_create`.

---

## bdd unittest generate

Generate a unit test from a requirement's acceptance criteria and
stage it. Each Given/When/Then criterion becomes one test case with
the Given as setup, the When as the action, and the Then as the
assertion.

```text
Usage: bdd unittest generate [OPTIONS] <REQ_ID>
```

```bash
bdd unittest generate REQ-003
```

```json
{
  "target": "src/test/java/StringCalculatorTest.java",
  "staged": true,
  "source": "template",
  "summary": "Unit test for REQ-003 with 2 cases from its acceptance criteria.",
  "nextStep": "Review with 'bdd changes show', apply with 'bdd changes commit', then 'bdd test' to see RED."
}
```

The target file and framework follow the detected language:

| Language | Test framework | Typical target |
| --- | --- | --- |
| Java | JUnit | `src/test/java/<Name>Test.java` |
| JavaScript | node test runner | `test/<name>.test.js` |
| TypeScript | node test runner + ts | `test/<name>.test.ts` |
| .NET | xUnit-style via the test project | `<Name>Tests.cs` |
| Rust | `#[test]` | `tests/<name>_test.rs` |

`source` works exactly as in
[`bdd steps generate`](steps.md#source-template-or-llm): deterministic
template by default, `"llm"` only when a model's polished version
passed validation, with the session language's best practices pinned
in the prompt. Generated assertions fail honestly until the
production code exists — the point is a real RED.

An unknown requirement id fails with exit status 1.

## Where it fits

```bash
bdd spec show REQ-003          # read the criteria
bdd unittest generate REQ-003  # stage the test
bdd changes commit
bdd test                       # RED at both altitudes
```

## See also

- [`bdd spec show`](spec.md#bdd-spec-show) — the criteria being turned into cases.
- [`bdd test`](test.md) — run the generated test.
