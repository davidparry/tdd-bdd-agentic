package com.davidparry.workshop.mcp.server.tdd;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class TddStateMachineTest {

    private final TddStateMachine machine = new TddStateMachine();

    private static TestRunSummary failing() {
        return new TestRunSummary(3, 1, 0, 0, List.of("StringCalculatorTest.addsTwoNumbers: expected 3 but was 1"));
    }

    private static TestRunSummary passing() {
        return new TestRunSummary(3, 0, 0, 0, List.of());
    }

    @Test
    @DisplayName("starts in START with no recorded runs")
    void startsInStartPhase() {
        assertThat(machine.phase()).isEqualTo(TddPhase.START);
        assertThat(machine.lastRun().tests()).isZero();
    }

    @Test
    @DisplayName("a failing test run moves the cycle to RED")
    void failingRunMovesToRed() {
        assertThat(machine.recordTestRun(failing())).isEqualTo(TddPhase.RED);
        assertThat(machine.lastRun().failureDetails()).hasSize(1);
    }

    @Test
    @DisplayName("a passing test run moves the cycle to GREEN")
    void passingRunMovesToGreen() {
        machine.recordTestRun(failing());
        assertThat(machine.recordTestRun(passing())).isEqualTo(TddPhase.GREEN);
    }

    @Test
    @DisplayName("a run with zero tests does not count as GREEN")
    void zeroTestsIsNotGreen() {
        assertThat(machine.recordTestRun(TestRunSummary.empty())).isEqualTo(TddPhase.RED);
    }

    @Test
    @DisplayName("refactoring may begin from GREEN")
    void refactorAllowedFromGreen() {
        machine.recordTestRun(passing());
        assertThat(machine.startRefactor("extract delimiter parsing")).isEqualTo(TddPhase.REFACTOR);
        assertThat(machine.refactorLog()).containsExactly("extract delimiter parsing");
    }

    @Test
    @DisplayName("refactoring on a red bar is rejected")
    void refactorRejectedFromRed() {
        machine.recordTestRun(failing());
        assertThatThrownBy(() -> machine.startRefactor("cleanup"))
                .isInstanceOf(IllegalStateException.class)
                .hasMessageContaining("red bar");
    }

    @Test
    @DisplayName("a passing run after a refactor returns to GREEN")
    void refactorThenPassingRunReturnsToGreen() {
        machine.recordTestRun(passing());
        machine.startRefactor("rename variables");
        assertThat(machine.recordTestRun(passing())).isEqualTo(TddPhase.GREEN);
    }

    @Test
    @DisplayName("a blank refactor note is recorded as '(no note)' and others are trimmed")
    void blankNoteIsNormalized() {
        machine.recordTestRun(passing());
        machine.startRefactor("   ");
        machine.recordTestRun(passing());
        machine.startRefactor("  tidy imports  ");
        assertThat(machine.refactorLog()).containsExactly("(no note)", "tidy imports");
    }

    @Test
    @DisplayName("every phase offers a next-step suggestion")
    void everyPhaseHasSuggestion() {
        assertThat(machine.suggestion()).isNotBlank();
        machine.recordTestRun(failing());
        assertThat(machine.suggestion()).containsIgnoringCase("failing");
        machine.recordTestRun(passing());
        assertThat(machine.suggestion()).containsIgnoringCase("pass");
        machine.startRefactor(null);
        assertThat(machine.suggestion()).containsIgnoringCase("refactor");
    }
}
