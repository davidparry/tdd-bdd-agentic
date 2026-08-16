package com.davidparry.workshop.mcp.server.tdd;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;

class TestRunSummaryTest {

    @Test
    @DisplayName("a run passes only when it has tests and no failures or errors")
    void passedRequiresTestsAndNoProblems() {
        assertThat(new TestRunSummary(3, 0, 0, 0, List.of()).passed()).isTrue();
        assertThat(new TestRunSummary(0, 0, 0, 0, List.of()).passed()).isFalse();
        assertThat(new TestRunSummary(3, 1, 0, 0, List.of("f")).passed()).isFalse();
        assertThat(new TestRunSummary(3, 0, 1, 0, List.of("e")).passed()).isFalse();
    }

    @Test
    @DisplayName("the empty summary has no tests and no details")
    void emptySummary() {
        TestRunSummary empty = TestRunSummary.empty();
        assertThat(empty.tests()).isZero();
        assertThat(empty.skipped()).isZero();
        assertThat(empty.failureDetails()).isEmpty();
    }
}
