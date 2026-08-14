package com.davidparry.workshop.mcp.server.tdd;

import java.util.ArrayList;
import java.util.List;

/**
 * Tracks the Red/Green/Refactor cycle across tool invocations.
 *
 * <p>Transitions:
 * <ul>
 *   <li>Any phase + failing test run → {@link TddPhase#RED}</li>
 *   <li>Any phase + passing test run → {@link TddPhase#GREEN}</li>
 *   <li>{@link TddPhase#GREEN} + refactor started → {@link TddPhase#REFACTOR}</li>
 * </ul>
 * Refactoring may only begin from GREEN — you never refactor on a red bar.
 */
public class TddStateMachine {

    private TddPhase phase = TddPhase.START;
    private TestRunSummary lastRun = TestRunSummary.empty();
    private final List<String> refactorLog = new ArrayList<>();

    public synchronized TddPhase recordTestRun(TestRunSummary summary) {
        this.lastRun = summary;
        this.phase = summary.passed() ? TddPhase.GREEN : TddPhase.RED;
        return phase;
    }

    public synchronized TddPhase startRefactor(String note) {
        if (phase != TddPhase.GREEN) {
            throw new IllegalStateException(
                    "Refactoring is only allowed from GREEN (current phase: " + phase
                            + "). Never refactor on a red bar — make the tests pass first.");
        }
        refactorLog.add(note == null || note.isBlank() ? "(no note)" : note.trim());
        phase = TddPhase.REFACTOR;
        return phase;
    }

    public synchronized TddPhase phase() {
        return phase;
    }

    public synchronized TestRunSummary lastRun() {
        return lastRun;
    }

    public synchronized List<String> refactorLog() {
        return List.copyOf(refactorLog);
    }

    /** A human/agent-readable hint about what to do next. */
    public synchronized String suggestion() {
        return switch (phase) {
            case START -> "No tests have been run yet. Call run_tests to establish a baseline.";
            case RED -> "Tests are failing. Write the simplest production code that makes them pass, then call run_tests again.";
            case GREEN -> "All tests pass. Either call start_refactor to clean up, or call get_requirement for the next pending requirement and write a failing test for it.";
            case REFACTOR -> "A refactor is in progress. Call run_tests to prove the refactor kept the bar green.";
        };
    }
}
