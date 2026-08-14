package com.davidparry.workshop.kata;

import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;

/**
 * The String Calculator kata — the code under test for this workshop.
 *
 * <p>This is the <strong>complete</strong> branch: every requirement in
 * {@code requirements/requirements.json} (the SDD spec) has been driven
 * through the full loop — the acceptance criteria became executable Gherkin
 * scenarios (BDD) and unit tests (TDD), then the behavior was implemented
 * through Red/Green/Refactor. Compare with {@code trunk}, the workshop
 * starting point.
 */
public class StringCalculator {

    /**
     * Adds the numbers in a delimited string.
     *
     * @param input numbers separated by commas or newlines, e.g. "" or "1"
     *              or "1,2" or "1\n2,3"
     * @return the sum of the numbers; 0 for an empty string
     * @throws IllegalArgumentException if any number is negative; the message
     *                                  lists every negative number found
     */
    public int add(String input) {
        // REQ-001: empty (or null) input is safe and sums to zero.
        if (input == null || input.isBlank()) {
            return 0;
        }
        // REQ-002/003/004: one or more numbers; REQ-005/007: newline or comma delimiters.
        List<Integer> negatives = new ArrayList<>();
        int sum = 0;
        for (String part : input.split("[,\n]")) {
            int value = Integer.parseInt(part.trim());
            if (value < 0) {
                negatives.add(value);
            }
            sum += value;
        }
        // REQ-006: negatives are rejected, and every offender is reported.
        if (!negatives.isEmpty()) {
            throw new IllegalArgumentException("negatives not allowed: "
                    + negatives.stream().map(String::valueOf).collect(Collectors.joining(", ")));
        }
        return sum;
    }
}
