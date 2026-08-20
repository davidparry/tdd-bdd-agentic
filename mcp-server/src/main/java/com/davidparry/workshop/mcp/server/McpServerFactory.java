package com.davidparry.workshop.mcp.server;

import io.modelcontextprotocol.server.McpServer;
import io.modelcontextprotocol.server.McpServerFeatures.SyncToolSpecification;
import io.modelcontextprotocol.server.McpSyncServer;
import io.modelcontextprotocol.spec.McpSchema.ServerCapabilities;
import io.modelcontextprotocol.spec.McpSchema.Tool;
import io.modelcontextprotocol.spec.McpServerTransportProvider;

import java.util.List;
import java.util.Map;

/**
 * Assembles the workshop MCP server: tool schemas, server metadata, and
 * capabilities. All tool logic is injected as {@link WorkflowToolHandlers};
 * the transport is injected so the assembly is unit-testable without stdio.
 */
public final class McpServerFactory {

    private McpServerFactory() {
    }

    /** The workflow tools, in workflow order, each delegating to an injected handler. */
    public static List<SyncToolSpecification> toolSpecifications(WorkflowToolHandlers handlers) {
        return List.of(
                new SyncToolSpecification(
                        Tool.builder("list_requirements", noArgsSchema())
                                .description("List every requirement of the kata with its id, title, and "
                                        + "implementation status. Use this to find pending work.")
                                .build(),
                        (exchange, request) -> handlers.listRequirements()),
                new SyncToolSpecification(
                        Tool.builder("get_requirement", Map.of(
                                        "type", "object",
                                        "properties", Map.of(
                                                "id", Map.of(
                                                        "type", "string",
                                                        "description", "The requirement id, e.g. REQ-003")),
                                        "required", List.of("id")))
                                .description("Get the user story and acceptance criteria for one requirement. "
                                        + "Turn each acceptance criterion into a failing JUnit test in "
                                        + "kata/src/test/java before writing production code.")
                                .build(),
                        (exchange, request) -> handlers.getRequirement(request.arguments())),
                new SyncToolSpecification(
                        Tool.builder("validate_spec", noArgsSchema())
                                .description("Validate the requirements spec on disk. Call this after every "
                                        + "edit to the requirements file and fix the reported issues until "
                                        + "valid is true — only a valid spec is worth turning into scenarios "
                                        + "and code. Implemented requirements must have tagged Gherkin "
                                        + "scenarios in their feature file.")
                                .build(),
                        (exchange, request) -> handlers.validateSpec()),
                new SyncToolSpecification(
                        Tool.builder("refine_requirement", Map.of(
                                        "type", "object",
                                        "properties", Map.of(
                                                "id", Map.of(
                                                        "type", "string",
                                                        "description", "The requirement id, e.g. REQ-007")),
                                        "required", List.of("id")))
                                .description("Review one requirement's wording for quality: ambiguous "
                                        + "language, a story missing its actor or rationale, outcomes that "
                                        + "are not measurable, criteria covering more than one action, and "
                                        + "missing edge cases. Reword the requirement from the findings and "
                                        + "call again - iterate until there are no findings, then have the "
                                        + "developer approve the wording before writing any scenario.")
                                .build(),
                        (exchange, request) -> handlers.refineRequirement(request.arguments())),
                new SyncToolSpecification(
                        Tool.builder("run_tests", noArgsSchema())
                                .description("Run the kata test suite with Maven and report the outcome. "
                                        + "Updates the Red/Green/Refactor state: failures mean RED, "
                                        + "all-passing means GREEN.")
                                .build(),
                        (exchange, request) -> handlers.runTests()),
                new SyncToolSpecification(
                        Tool.builder("get_tdd_state", noArgsSchema())
                                .description("Get the current phase of the Red/Green/Refactor cycle, the last "
                                        + "test run summary, and a suggested next step.")
                                .build(),
                        (exchange, request) -> handlers.getTddState()),
                new SyncToolSpecification(
                        Tool.builder("start_refactor", Map.of(
                                        "type", "object",
                                        "properties", Map.of(
                                                "note", Map.of(
                                                        "type", "string",
                                                        "description", "What you intend to refactor and why"))))
                                .description("Begin a refactor step. Only allowed when the bar is GREEN — "
                                        + "never refactor on failing tests. Run run_tests afterwards to "
                                        + "prove the refactor was safe.")
                                .build(),
                        (exchange, request) -> handlers.startRefactor(request.arguments())));
    }

    /** Builds the server on the given transport with the given handlers. */
    public static McpSyncServer create(McpServerTransportProvider transportProvider,
                                       WorkflowToolHandlers handlers) {
        return McpServer.sync(transportProvider)
                .serverInfo("tdd-workflow-server", "1.0.0")
                .instructions("""
                        Drives a spec-driven TDD/BDD workflow for the String Calculator kata.
                        The requirements spec is the source of truth and the entry point. Spec
                        iteration loop: draft or edit a requirement in the requirements file ->
                        validate_spec until the spec is valid -> refine_requirement on the new or
                        changed requirement and reword from its findings until there are none ->
                        have the developer approve the wording. From an approved spec:
                        list_requirements -> get_requirement (pick a pending one) -> write a
                        Gherkin scenario from its acceptance criteria in the feature file (BDD
                        level), add step definitions if needed, and/or a failing JUnit test (unit
                        level) -> run_tests (expect RED) -> implement -> run_tests (expect GREEN)
                        -> start_refactor -> run_tests. The human developer stays in control of
                        every engineering decision.""")
                .capabilities(ServerCapabilities.builder()
                        .tools(true)
                        .logging()
                        .build())
                .tools(toolSpecifications(handlers))
                .build();
    }

    private static Map<String, Object> noArgsSchema() {
        return Map.of("type", "object", "properties", Map.of());
    }
}
