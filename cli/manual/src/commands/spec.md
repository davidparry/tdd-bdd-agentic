# bdd spec

Requirements spec tools. The spec at
`requirements/requirements.json` is the project's source of truth;
these subcommands read it, gate it, and mutate it (through the
[staging area](../staged-changes.md)).

```text
Usage: bdd spec [OPTIONS] <COMMAND>

Commands: list, show, draft, validate, refine, mark-implemented
```

MCP tool equivalents: `list_requirements`, `get_requirement`,
`validate_spec`, `refine_requirement`, `requirement_mark_implemented`.

---

## bdd spec list

List every requirement with its id, title, and status.

```bash
bdd spec list
```

```json
[
  { "id": "REQ-001", "title": "Empty string returns zero", "status": "implemented" },
  { "id": "REQ-002", "title": "A single number is returned as-is", "status": "pending" },
  { "id": "REQ-003", "title": "Two numbers separated by a comma are summed", "status": "pending" }
]
```

Use it to pick the next `pending` requirement to work on.

---

## bdd spec show

Show one requirement, enriched with file locations and a workflow
hint.

```text
Usage: bdd spec show [OPTIONS] <REQ_ID>
```

```bash
bdd spec show REQ-003
```

```json
{
  "id": "REQ-003",
  "title": "Two numbers separated by a comma are summed",
  "status": "pending",
  "story": "As a user, I want comma-separated numbers to be summed so that I can add multiple values at once.",
  "acceptanceCriteria": [
    "Given \"1,2\", when add is called, then the result is 3",
    "Given \"10,20\", when add is called, then the result is 30"
  ],
  "featureLocation": "features/string_calculator.feature",
  "stepDefinitions": "features/step_definitions/calculator_steps.js",
  "testLocation": "test/calculator.test.js",
  "productionLocation": "src/calculator.js",
  "workflowHint": "Write the Gherkin scenario for this requirement in the feature file first (tag it @REQ-003), reuse or add step definitions, then run tests to see RED."
}
```

An unknown id fails with exit status 1:

```text
Error: no requirement with id REQ-999
```

---

## bdd spec draft

Interactively draft a requirement. Human input drives the spec — the
CLI never invents requirements. The draft is validated and
quality-gated in a loop until it is clean, then staged.

```bash
bdd spec draft
```

The prompts, first pass:

```text
REQ-004 title:
REQ-004 story:
Acceptance criteria (Given/When/Then). A blank criterion ends the list:
REQ-004 criterion 1 (leave blank to finish the criteria):
REQ-004 criterion 2 (leave blank to finish the criteria):
```

- The id is allocated automatically (next free `REQ-nnn`).
- A **blank criterion ends the list** — enter at least one first.
- On a color console the bracketed suggestion — the text
  <kbd>Enter</kbd> will use — renders green; the destructive
  `'-' drops it` hint on criterion prompts, model failures, and other
  dead ends render red; the animated dots on `working ...` lines
  render light yellow.
- On a real terminal answers are edited on a `> ` line with full line
  editing — arrow keys move the cursor anywhere in the typed text,
  Home/End jump, and the up arrow recalls this session's answers.

If validation or refinement finds problems, each finding prints with a
concrete suggestion, and the rewording pass shows your prior answers:

```text
REQ-004: acceptance criterion 1 must be phrased Given/When/Then
  try: rephrase as: Given <starting state>, when <action>, then <exact result>

REQ-004 title [Newlines act as delimiters] (Enter keeps it):
REQ-004 story [As a user, I want newlines to work like commas so that input can be multi-line.] (Enter keeps it):
REQ-004 criterion 1 [it should handle newlines] (Enter keeps it, '-' drops it):
Given "1\n2", when add is called, then the result is 3
REQ-004 criterion 2 (leave blank to finish the criteria):
```

- <kbd>Enter</kbd> keeps the prior answer.
- `-` drops a prior criterion.
- New criteria can be appended after the priors.

With a resolved model, `bdd spec draft` runs the same
description-driven wizard as [greenfield](greenfield.md#the-description-driven-wizard),
and findings are sent to the model first: the rewording prompts carry
its corrected proposal instead of your raw prior answers, so
<kbd>Enter</kbd> accepts each fix (for the happy-paths finding, that
includes the edge-case criterion it added):

```text
Findings to address:
  - criteria: only happy paths - add at least one edge case (empty, invalid, or error input)
    try: add an edge case, e.g. Given an empty string "", when add is called, then the result is 0
Asking qwen3-coder-next:latest to address finding 1 of 1 - working ...
The model reworded the draft. Each prompt shows its proposal - Enter accepts it, or type your own wording.
REQ-004 title [Newlines act as delimiters] (Enter keeps it):
REQ-004 criterion 2 [Given an empty string "", when add is called, then the result is 0] (Enter keeps it, '-' drops it):
```

Each finding is its own model call: call 1 addresses finding 1 on your
draft, call 2 addresses finding 2 on the draft call 1 produced, and so
on — the fixes accumulate one finding at a time instead of asking for
everything at once. An unusable reply skips that finding (it stays
yours to fix at the prompts); a model error ends the chain and keeps
whatever fixes already landed.

If the review rejects a wording again, the next model call also
recounts every earlier wording of the draft and the findings each one
produced, so the model never proposes a wording the review already
rejected.

On a terminal the `working ...` lines animate — the trailing dots grow
`.` `..` `...` in light yellow and start over — while the model call
is in flight.

If the model is unreachable or its rewording is unusable, the prompts
fall back to your prior answers exactly as without a model. Nothing is
accepted silently either way — every field still passes through your
hands, and validate + refine rerun on whatever you accept.

When the draft is clean it is staged:

```json
{
  "id": "REQ-004",
  "title": "Newlines act as delimiters",
  "staged": true,
  "nextStep": "Review with 'bdd changes show', apply with 'bdd changes commit', then write the Gherkin scenario."
}
```

---

## bdd spec validate

Validate the whole spec on disk: JSON shape, required fields, unique
ids, status values, and Given/When/Then phrasing of every criterion.

```bash
bdd spec validate
```

Valid spec:

```json
{
  "valid": true,
  "issues": [],
  "nextStep": "Pick a pending requirement with 'bdd spec list' and write its scenario."
}
```

Invalid spec (the command still exits 0 — the *report* carries the
verdict):

```json
{
  "valid": false,
  "issues": [
    "REQ-005: acceptance criterion 1 must be phrased Given/When/Then ('the calculator should handle newlines quickly')"
  ],
  "nextStep": "Fix the issues in requirements/requirements.json, then validate again."
}
```

---

## bdd spec refine

Review one requirement's wording for quality: vague words, missing
actor or benefit in the story, compound criteria, non-concrete
outcomes, happy-path-only criteria.

```text
Usage: bdd spec refine [OPTIONS] <REQ_ID>
```

```bash
bdd spec refine REQ-004
```

```json
{
  "id": "REQ-004",
  "clean": false,
  "findings": [
    "acceptance criterion 1 is ambiguous: 'quickly' is not testable",
    "the story is missing the why (no 'so that ...')"
  ],
  "nextStep": "Reword the flagged parts, then refine again until there are no findings."
}
```

Refine until `"clean": true`, then have the developer approve the
wording — that approval is the first human gate of the workflow.

---

## bdd spec mark-implemented

Flip a requirement's status to `implemented` and record its
`featureFile` — the feature carrying the `@REQ-...` tag — in the same
staged edit, so the spec passes validation. The change is staged, not
applied directly.

```text
Usage: bdd spec mark-implemented [OPTIONS] <REQ_ID>
```

```bash
bdd spec mark-implemented REQ-003
bdd validate
bdd changes commit
```

```json
{
  "id": "REQ-003",
  "status": "implemented",
  "staged": true,
  "nextStep": "Review with changes show, run bdd validate (it checks the @REQ-003 scenario exists), then bdd changes commit."
}
```

The command is gated twice:

- **GREEN only** — it refuses unless the last recorded run passed.
  Do this only when the scenario and tests for the requirement are
  GREEN; it is the last move of the per-requirement rhythm.
- **Tagged scenario only** — it refuses when no committed feature
  file carries a scenario tagged `@<REQ_ID>`; add one with
  `bdd scenario add` and apply it with `bdd changes commit` first.

Re-running it on an already-implemented requirement is safe: on GREEN
it backfills a missing `featureFile`, which is exactly the repair for
a spec that fails validation with
`implemented requirements must name their featureFile`.

## See also

- [The workflow](../workflow.md) — where each subcommand fits.
- [`bdd changes`](changes.md) — reviewing and applying staged spec mutations.
