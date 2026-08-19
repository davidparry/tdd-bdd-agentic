package com.davidparry.workshop.mcp.server;

import com.davidparry.workshop.mcp.server.requirements.Requirement;
import com.davidparry.workshop.mcp.server.requirements.RequirementRefiner;
import com.davidparry.workshop.mcp.server.requirements.RequirementsRepository;
import com.davidparry.workshop.mcp.server.requirements.SpecValidator;
import com.davidparry.workshop.mcp.server.tdd.TddStateMachine;
import com.davidparry.workshop.mcp.server.tdd.TestRunner;
import com.davidparry.workshop.mcp.server.tdd.TestRunSummary;
import tools.jackson.databind.ObjectMapper;
import tools.jackson.databind.SerializationFeature;
import tools.jackson.databind.json.JsonMapper;

import io.modelcontextprotocol.spec.McpSchema;
import io.modelcontextprotocol.spec.McpSchema.CallToolResult;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;

/**
 * The logic behind each of the server's five tools, extracted from the
 * transport wiring in {@link TddMcpServer} so every handler can be
 * unit-tested without a running MCP session.
 */
public class WorkflowToolHandlers {

    private static final ObjectMapper JSON = JsonMapper.builder()
            .enable(SerializationFeature.INDENT_OUTPUT)
            .build();

    /**
     * The spec is re-read on every call so agents that edit
     * {@code requirements.json} mid-session always see their latest version.
     */
    private final Supplier<RequirementsRepository> requirementsSource;
    private final SpecValidator specValidator;
    private final RequirementRefiner refiner;
    private final TestRunner testRunner;
    private final TddStateMachine tdd;

    public WorkflowToolHandlers(Supplier<RequirementsRepository> requirementsSource,
                                SpecValidator specValidator,
                                RequirementRefiner refiner,
                                TestRunner testRunner,
                                TddStateMachine tdd) {
        this.requirementsSource = requirementsSource;
        this.specValidator = specValidator;
        this.refiner = refiner;
        this.testRunner = testRunner;
        this.tdd = tdd;
    }

    /** Handler for {@code list_requirements}. */
    public CallToolResult listRequirements() {
        RequirementsRepository requirements = requirementsSource.get();
        List<Map<String, Object>> rows = requirements.all().stream()
                .map(r -> {
                    Map<String, Object> row = new LinkedHashMap<String, Object>();
                    row.put("id", r.id());
                    row.put("title", r.title());
                    row.put("status", r.status());
                    return row;
                })
                .toList();
        Map<String, Object> body = new LinkedHashMap<>();
        body.put("project", requirements.projectName());
        body.put("requirements", rows);
        return textResult(body);
    }

    /** Handler for {@code get_requirement}. */
    public CallToolResult getRequirement(Map<String, Object> arguments) {
        String id = String.valueOf(arguments.get("id"));
        return requirementsSource.get().byId(id)
                .map(WorkflowToolHandlers::requirementDetail)
                .map(WorkflowToolHandlers::textResult)
                .orElseGet(() -> errorResult("No requirement with id '" + id
                        + "'. Call list_requirements to see valid ids."));
    }

    /** Handler for {@code validate_spec}. */
    public CallToolResult validateSpec() {
        List<String> issues = specValidator.validate();
        boolean valid = issues.isEmpty();
        Map<String, Object> body = new LinkedHashMap<>();
        body.put("valid", valid);
        body.put("issues", issues);
        body.put("nextStep", valid
                ? "The spec is valid. Call get_requirement for a pending requirement and write its "
                        + "Gherkin scenario from the acceptance criteria."
                : "Fix the issues in the requirements file, then call validate_spec again. "
                        + "Iterate until valid is true before writing scenarios or code.");
        return textResult(body);
    }

    /** Handler for {@code refine_requirement}. */
    public CallToolResult refineRequirement(Map<String, Object> arguments) {
        String id = String.valueOf(arguments.get("id"));
        return requirementsSource.get().byId(id)
                .map(this::refinementDetail)
                .map(WorkflowToolHandlers::textResult)
                .orElseGet(() -> errorResult("No requirement with id '" + id
                        + "'. Call list_requirements to see valid ids."));
    }

    private Map<String, Object> refinementDetail(Requirement requirement) {
        List<String> findings = refiner.review(requirement);
        boolean clean = findings.isEmpty();
        Map<String, Object> body = new LinkedHashMap<>();
        body.put("id", requirement.id());
        body.put("clean", clean);
        body.put("findings", findings);
        body.put("nextStep", clean
                ? "The wording reads clean. Confirm it with the developer, then write the "
                        + "Gherkin scenario from the acceptance criteria."
                : "Refine the wording in the requirements file to address each finding, run "
                        + "validate_spec, then call refine_requirement again. Iterate until "
                        + "there are no findings.");
        return body;
    }

    /** Handler for {@code run_tests}. */
    public CallToolResult runTests() {
        TestRunSummary summary = testRunner.runKataTests();
        var phase = tdd.recordTestRun(summary);
        Map<String, Object> body = new LinkedHashMap<>();
        body.put("phase", phase.name());
        body.put("tests", summary.tests());
        body.put("failures", summary.failures());
        body.put("errors", summary.errors());
        body.put("skipped", summary.skipped());
        body.put("failureDetails", summary.failureDetails());
        body.put("nextStep", tdd.suggestion());
        return textResult(body);
    }

    /** Handler for {@code get_tdd_state}. */
    public CallToolResult getTddState() {
        TestRunSummary last = tdd.lastRun();
        Map<String, Object> body = new LinkedHashMap<>();
        body.put("phase", tdd.phase().name());
        body.put("lastRun", Map.of(
                "tests", last.tests(),
                "failures", last.failures(),
                "errors", last.errors(),
                "skipped", last.skipped()));
        body.put("refactorLog", tdd.refactorLog());
        body.put("nextStep", tdd.suggestion());
        return textResult(body);
    }

    /** Handler for {@code start_refactor}. */
    public CallToolResult startRefactor(Map<String, Object> arguments) {
        Object note = arguments == null ? null : arguments.get("note");
        try {
            var phase = tdd.startRefactor(note == null ? null : note.toString());
            return textResult(Map.of(
                    "phase", phase.name(),
                    "nextStep", tdd.suggestion()));
        } catch (IllegalStateException e) {
            return errorResult(e.getMessage());
        }
    }

    private static Map<String, Object> requirementDetail(Requirement r) {
        Map<String, Object> body = new LinkedHashMap<>();
        body.put("id", r.id());
        body.put("title", r.title());
        body.put("status", r.status());
        body.put("story", r.story());
        body.put("acceptanceCriteria", r.acceptanceCriteria());
        if (r.featureFile() != null) {
            body.put("featureLocation", r.featureFile());
        }
        body.put("stepDefinitions", "kata/src/test/java/com/davidparry/workshop/kata/StringCalculatorSteps.java");
        body.put("testLocation", "kata/src/test/java/com/davidparry/workshop/kata/StringCalculatorTest.java");
        body.put("productionLocation", "kata/src/main/java/com/davidparry/workshop/kata/StringCalculator.java");
        body.put("workflowHint", "Write the Gherkin scenario for this requirement in the feature file first "
                + "(tag it @" + r.id() + "), reuse or add step definitions, then run_tests to see RED.");
        return body;
    }

    private static CallToolResult textResult(Map<String, Object> body) {
        return CallToolResult.builder()
                .content(List.of(new McpSchema.TextContent(JSON.writeValueAsString(body))))
                .isError(false)
                .build();
    }

    private static CallToolResult errorResult(String message) {
        return CallToolResult.builder()
                .content(List.of(new McpSchema.TextContent(message)))
                .isError(true)
                .build();
    }
}
