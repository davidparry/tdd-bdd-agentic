package com.davidparry.workshop.mcp.server.tdd;

/**
 * Runs the kata test suite and summarizes the outcome. The MCP tool handlers
 * depend on this abstraction; {@link MavenTestRunner} is the production
 * implementation.
 */
@FunctionalInterface
public interface TestRunner {

    TestRunSummary runKataTests();
}
