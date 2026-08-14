package com.davidparry.workshop.kata;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

/**
 * Unit-level (TDD) tests generated from requirements during the workshop.
 * The behavior-level (BDD) spec lives in
 * {@code src/test/resources/features/string_calculator.feature} and runs
 * through Cucumber via {@link RunCucumberTest}.
 *
 * <p>Convention: tests are grouped and named by requirement ID. The MCP
 * server's {@code run_tests} tool executes both suites together and reports
 * one combined RED/GREEN result back to the agent and developer.
 */
class StringCalculatorTest {

    private final StringCalculator calculator = new StringCalculator();

    @Test
    @DisplayName("REQ-001: an empty string returns 0")
    void emptyStringReturnsZero() {
        assertThat(calculator.add("")).isZero();
    }

    @Test
    @DisplayName("REQ-002: a single number returns its value")
    void singleNumberReturnsItsValue() {
        assertThat(calculator.add("7")).isEqualTo(7);
    }

    @Test
    @DisplayName("REQ-003: two comma-separated numbers are summed")
    void twoCommaSeparatedNumbersAreSummed() {
        assertThat(calculator.add("1,2")).isEqualTo(3);
        assertThat(calculator.add("10,20")).isEqualTo(30);
    }

    @Test
    @DisplayName("REQ-004: any amount of numbers is summed")
    void anyAmountOfNumbersIsSummed() {
        assertThat(calculator.add("1,2,3,4,5")).isEqualTo(15);
        assertThat(calculator.add("0,0,0")).isZero();
    }

    @Test
    @DisplayName("REQ-005/REQ-007: newlines work as delimiters alongside commas")
    void newlinesWorkAsDelimiters() {
        assertThat(calculator.add("1\n2,3")).isEqualTo(6);
        assertThat(calculator.add("4\n5\n6")).isEqualTo(15);
    }

    @Test
    @DisplayName("REQ-006: a negative number is rejected with a clear message")
    void negativeNumberIsRejected() {
        assertThatThrownBy(() -> calculator.add("1,-2"))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("negatives not allowed");
    }

    @Test
    @DisplayName("REQ-006: every negative number is listed in the error message")
    void everyNegativeNumberIsListed() {
        assertThatThrownBy(() -> calculator.add("-1,-2"))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("-1")
                .hasMessageContaining("-2");
    }
}
