package com.davidparry.workshop.mcp.server.tdd;

import java.util.List;

/**
 * The outcome of a single execution of the kata test suite.
 */
public record TestRunSummary(
        int tests,
        int failures,
        int errors,
        int skipped,
        List<String> failureDetails) {

    public TestRunSummary {
        failureDetails = List.copyOf(failureDetails);
    }

    public static TestRunSummary empty() {
        return new TestRunSummary(0, 0, 0, 0, List.of());
    }

    public boolean passed() {
        return tests > 0 && failures == 0 && errors == 0;
    }
}
