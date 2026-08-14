package com.davidparry.workshop.mcp.server;

import com.davidparry.workshop.mcp.server.requirements.RequirementRefiner;
import com.davidparry.workshop.mcp.server.requirements.RequirementsRepository;
import com.davidparry.workshop.mcp.server.requirements.SpecValidator;
import com.davidparry.workshop.mcp.server.tdd.TddStateMachine;
import com.davidparry.workshop.mcp.server.tdd.TestRunner;
import com.davidparry.workshop.mcp.server.tdd.TestRunSummary;

import io.modelcontextprotocol.spec.McpSchema;
import io.modelcontextprotocol.spec.McpSchema.CallToolResult;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.assertj.core.api.Assertions.assertThat;

class WorkflowToolHandlersTest {

    @TempDir
    Path dir;

    private TddStateMachine tdd;
    private StubTestRunner testRunner;
    private WorkflowToolHandlers handlers;

    /** Replaces the real Maven invocation with a canned summary. */
    private static final class StubTestRunner implements TestRunner {
        private TestRunSummary next = TestRunSummary.empty();

        @Override
        public TestRunSummary runKataTests() {
            return next;
        }
    }

    @BeforeEach
    void setUp() throws IOException {
        Path file = dir.resolve("requirements.json");
        Files.writeString(file, """
                {
                  "project": "Test Kata",
                  "requirements": [
                    {
                      "id": "REQ-001",
                      "title": "First",
                      "status": "implemented",
                      "story": "As a user...",
                      "acceptanceCriteria": ["Given x, then y"]
                    },
                    {
                      "id": "REQ-002",
                      "title": "Second",
                      "status": "pending",
                      "story": "As a user...",
                      "acceptanceCriteria": ["Given a, then b"],
                      "featureFile": "kata/src/test/resources/features/string_calculator.feature"
                    }
                  ]
                }
                """);
        tdd = new TddStateMachine();
        testRunner = new StubTestRunner();
        handlers = new WorkflowToolHandlers(
                () -> RequirementsRepository.load(file),
                new SpecValidator(file, dir),
                new RequirementRefiner(),
                testRunner,
                tdd);
    }

    private static String text(CallToolResult result) {
        return ((McpSchema.TextContent) result.content().get(0)).text();
    }

    @Test
    @DisplayName("list_requirements returns every requirement with id, title, and status")
    void listRequirementsReturnsAll() {
        CallToolResult result = handlers.listRequirements();
        assertThat(result.isError()).isFalse();
        assertThat(text(result))
                .contains("Test Kata")
                .contains("REQ-001").contains("First").contains("implemented")
                .contains("REQ-002").contains("Second").contains("pending");
    }

    @Test
    @DisplayName("get_requirement returns story, criteria, and workflow hint")
    void getRequirementReturnsDetail() {
        CallToolResult result = handlers.getRequirement(Map.of("id", "REQ-002"));
        assertThat(result.isError()).isFalse();
        assertThat(text(result))
                .contains("Given a, then b")
                .contains("featureLocation")
                .contains("workflowHint")
                .contains("@REQ-002");
    }

    @Test
    @DisplayName("get_requirement omits featureLocation when the spec has none")
    void getRequirementWithoutFeatureFile() {
        CallToolResult result = handlers.getRequirement(Map.of("id", "REQ-001"));
        assertThat(result.isError()).isFalse();
        assertThat(text(result)).doesNotContain("featureLocation");
    }

    @Test
    @DisplayName("get_requirement with an unknown id is an isError result, not an exception")
    void getRequirementUnknownId() {
        CallToolResult result = handlers.getRequirement(Map.of("id", "REQ-999"));
        assertThat(result.isError()).isTrue();
        assertThat(text(result)).contains("REQ-999").contains("list_requirements");
    }

    @Test
    @DisplayName("validate_spec reports issues so the agent can iterate")
    void validateSpecReportsIssues() {
        // The fixture spec is deliberately not yet valid: REQ-001 is
        // implemented without a featureFile and its criteria lack a When.
        CallToolResult result = handlers.validateSpec();
        assertThat(result.isError()).isFalse();
        assertThat(text(result))
                .contains("\"valid\" : false")
                .contains("must be phrased Given/When/Then")
                .contains("call validate_spec again");
    }

    @Test
    @DisplayName("validate_spec on a valid spec points at the next workflow step")
    void validateSpecValid() throws IOException {
        Path feature = dir.resolve("features").resolve("calc.feature");
        Files.createDirectories(feature.getParent());
        Files.writeString(feature, "@REQ-001\nScenario: done");
        Path file = dir.resolve("valid.json");
        Files.writeString(file, """
                {
                  "project": "Valid Kata",
                  "requirements": [
                    {
                      "id": "REQ-001",
                      "title": "First",
                      "status": "implemented",
                      "story": "As a user...",
                      "acceptanceCriteria": ["Given x, when y, then z"],
                      "featureFile": "features/calc.feature"
                    }
                  ]
                }
                """);
        WorkflowToolHandlers valid = new WorkflowToolHandlers(
                () -> RequirementsRepository.load(file),
                new SpecValidator(file, dir),
                new RequirementRefiner(),
                testRunner,
                tdd);
        CallToolResult result = valid.validateSpec();
        assertThat(result.isError()).isFalse();
        assertThat(text(result))
                .contains("\"valid\" : true")
                .contains("write its Gherkin scenario");
    }

    @Test
    @DisplayName("refine_requirement returns wording findings for the agent to act on")
    void refineRequirementFindings() {
        // The fixture's REQ-001 story has no actor and no 'so that'.
        CallToolResult result = handlers.refineRequirement(Map.of("id", "REQ-001"));
        assertThat(result.isError()).isFalse();
        assertThat(text(result))
                .contains("\"clean\" : false")
                .contains("call refine_requirement again");
    }

    @Test
    @DisplayName("refine_requirement reports clean wording and hands off to the scenario step")
    void refineRequirementClean() throws IOException {
        Path file = dir.resolve("clean.json");
        Files.writeString(file, """
                {
                  "project": "Clean Kata",
                  "requirements": [
                    {
                      "id": "REQ-001",
                      "title": "Commas",
                      "status": "pending",
                      "story": "As a calculator user, I want commas to separate numbers so that lists just work.",
                      "acceptanceCriteria": [
                        "Given the input \\"1,2\\", when add is called, then the result is 3",
                        "Given an empty string \\"\\", when add is called, then the result is 0"
                      ]
                    }
                  ]
                }
                """);
        WorkflowToolHandlers clean = new WorkflowToolHandlers(
                () -> RequirementsRepository.load(file),
                new SpecValidator(file, dir),
                new RequirementRefiner(),
                testRunner,
                tdd);
        CallToolResult result = clean.refineRequirement(Map.of("id", "REQ-001"));
        assertThat(result.isError()).isFalse();
        assertThat(text(result))
                .contains("\"clean\" : true")
                .contains("Confirm it with the developer");
    }

    @Test
    @DisplayName("refine_requirement for an unknown id is a tool error with guidance")
    void refineRequirementUnknownId() {
        CallToolResult result = handlers.refineRequirement(Map.of("id", "REQ-999"));
        assertThat(result.isError()).isTrue();
        assertThat(text(result)).contains("list_requirements");
    }

    @Test
    @DisplayName("run_tests reports GREEN when the suite passes")
    void runTestsGreen() {
        testRunner.next = new TestRunSummary(5, 0, 0, 0, List.of());
        CallToolResult result = handlers.runTests();
        assertThat(result.isError()).isFalse();
        assertThat(text(result)).contains("GREEN").contains("\"tests\" : 5");
    }

    @Test
    @DisplayName("run_tests reports RED with failure details when the suite fails")
    void runTestsRed() {
        testRunner.next = new TestRunSummary(5, 1, 0, 0, List.of("FooTest.a: expected 3 but was 1"));
        CallToolResult result = handlers.runTests();
        assertThat(result.isError()).isFalse();
        assertThat(text(result)).contains("RED").contains("expected 3 but was 1");
    }

    @Test
    @DisplayName("get_tdd_state reflects the last recorded run and the refactor log")
    void getTddStateReflectsRuns() {
        testRunner.next = new TestRunSummary(5, 0, 0, 0, List.of());
        handlers.runTests();
        tdd.startRefactor("extract parsing");
        CallToolResult result = handlers.getTddState();
        assertThat(result.isError()).isFalse();
        assertThat(text(result))
                .contains("REFACTOR")
                .contains("extract parsing")
                .contains("nextStep");
    }

    @Test
    @DisplayName("start_refactor succeeds from GREEN and records the note")
    void startRefactorFromGreen() {
        testRunner.next = new TestRunSummary(5, 0, 0, 0, List.of());
        handlers.runTests();
        CallToolResult result = handlers.startRefactor(Map.of("note", "inline temp variable"));
        assertThat(result.isError()).isFalse();
        assertThat(text(result)).contains("REFACTOR");
        assertThat(tdd.refactorLog()).containsExactly("inline temp variable");
    }

    @Test
    @DisplayName("start_refactor tolerates null and missing arguments")
    void startRefactorWithoutArguments() {
        testRunner.next = new TestRunSummary(5, 0, 0, 0, List.of());
        handlers.runTests();
        assertThat(handlers.startRefactor(null).isError()).isFalse();

        handlers.runTests();
        Map<String, Object> noNote = new HashMap<>();
        assertThat(handlers.startRefactor(noNote).isError()).isFalse();
        assertThat(tdd.refactorLog()).containsExactly("(no note)", "(no note)");
    }

    @Test
    @DisplayName("start_refactor on a red bar is an isError result carrying the rule")
    void startRefactorOnRedIsRefused() {
        testRunner.next = new TestRunSummary(5, 1, 0, 0, List.of("boom"));
        handlers.runTests();
        CallToolResult result = handlers.startRefactor(Map.of("note", "cleanup"));
        assertThat(result.isError()).isTrue();
        assertThat(text(result)).contains("red bar");
    }
}
