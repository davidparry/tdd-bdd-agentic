# This workshop’s String Calculator (CLI)

The 60-minute class uses Cursor and the Java MCP server. The same
kata can be finished with `bdd` instead. The student recipe — install,
branch, draft REQ-007, then drive REQ-003…007 to `implemented` — lives
in [student-follow-docs/cli-path.md](../../../student-follow-docs/cli-path.md).

Do not edit [student-follow-along.md](../../../student-follow-along.md)
for this path. Do not implement the kata on `trunk`.

Kata files `bdd` must reuse (not parallel `Req00NTest` / `Kata.java`
classes):

- `kata/src/test/resources/features/string_calculator.feature`
- `kata/src/test/java/com/davidparry/workshop/kata/StringCalculatorSteps.java`
- `kata/src/test/java/com/davidparry/workshop/kata/StringCalculatorTest.java`
- `kata/src/main/java/com/davidparry/workshop/kata/StringCalculator.java`

Commit staged files **before** [`bdd test`](commands/test.md). The
runner executes Maven on the working tree. [`bdd spec mark-implemented`](commands/spec.md)
stays GREEN-gated.
