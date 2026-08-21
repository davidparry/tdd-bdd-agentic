package com.davidparry.workshop.mcp.server.tdd;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.PosixFilePermissions;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class SurefireReportParserTest {

    @TempDir
    Path reportsDir;

    @Test
    @DisplayName("a missing reports directory yields an empty summary")
    void missingDirectoryYieldsEmptySummary() {
        TestRunSummary summary = SurefireReportParser.parse(reportsDir.resolve("does-not-exist"));
        assertThat(summary.tests()).isZero();
        assertThat(summary.passed()).isFalse();
    }

    @Test
    @DisplayName("a passing suite is aggregated with no failure details")
    void passingSuiteAggregates() throws IOException {
        write("TEST-com.example.FooTest.xml", """
                <?xml version="1.0" encoding="UTF-8"?>
                <testsuite name="com.example.FooTest" tests="2" failures="0" errors="0" skipped="0">
                  <testcase classname="com.example.FooTest" name="a" time="0.001"/>
                  <testcase classname="com.example.FooTest" name="b" time="0.001"/>
                </testsuite>
                """);
        TestRunSummary summary = SurefireReportParser.parse(reportsDir);
        assertThat(summary.tests()).isEqualTo(2);
        assertThat(summary.passed()).isTrue();
        assertThat(summary.failureDetails()).isEmpty();
    }

    @Test
    @DisplayName("failures across multiple suites are summed and detailed")
    void failuresAreSummedAndDetailed() throws IOException {
        write("TEST-com.example.FooTest.xml", """
                <?xml version="1.0" encoding="UTF-8"?>
                <testsuite name="com.example.FooTest" tests="2" failures="1" errors="0" skipped="0">
                  <testcase classname="com.example.FooTest" name="a" time="0.001"/>
                  <testcase classname="com.example.FooTest" name="b" time="0.001">
                    <failure message="expected 3 but was 1" type="AssertionError"/>
                  </testcase>
                </testsuite>
                """);
        write("TEST-com.example.BarTest.xml", """
                <?xml version="1.0" encoding="UTF-8"?>
                <testsuite name="com.example.BarTest" tests="1" failures="0" errors="1" skipped="0">
                  <testcase classname="com.example.BarTest" name="boom" time="0.001">
                    <error message="NullPointerException" type="java.lang.NullPointerException"/>
                  </testcase>
                </testsuite>
                """);
        TestRunSummary summary = SurefireReportParser.parse(reportsDir);
        assertThat(summary.tests()).isEqualTo(3);
        assertThat(summary.failures()).isEqualTo(1);
        assertThat(summary.errors()).isEqualTo(1);
        assertThat(summary.passed()).isFalse();
        assertThat(summary.failureDetails())
                .anyMatch(d -> d.contains("FooTest.b") && d.contains("expected 3 but was 1"))
                .anyMatch(d -> d.contains("BarTest.boom") && d.contains("NullPointerException"));
    }

    @Test
    @DisplayName("absent count attributes are treated as zero")
    void absentAttributesAreZero() throws IOException {
        write("TEST-com.example.FooTest.xml", """
                <?xml version="1.0" encoding="UTF-8"?>
                <testsuite name="com.example.FooTest" tests="1">
                  <testcase classname="com.example.FooTest" name="a" time="0.001"/>
                </testsuite>
                """);
        TestRunSummary summary = SurefireReportParser.parse(reportsDir);
        assertThat(summary.tests()).isEqualTo(1);
        assertThat(summary.skipped()).isZero();
        assertThat(summary.passed()).isTrue();
    }

    @Test
    @DisplayName("a failure without a message falls back to the tag name")
    void messagelessFailureUsesTagName() throws IOException {
        write("TEST-com.example.FooTest.xml", """
                <?xml version="1.0" encoding="UTF-8"?>
                <testsuite name="com.example.FooTest" tests="1" failures="1" errors="0" skipped="0">
                  <testcase classname="com.example.FooTest" name="a" time="0.001">
                    <failure type="AssertionError"/>
                  </testcase>
                </testsuite>
                """);
        TestRunSummary summary = SurefireReportParser.parse(reportsDir);
        assertThat(summary.failureDetails()).containsExactly("com.example.FooTest.a: failure");
    }

    @Test
    @DisplayName("files that are not TEST-*.xml reports are ignored")
    void nonReportFilesAreIgnored() throws IOException {
        write("README.txt", "not a report");
        write("TEST-com.example.FooTest.txt", "not xml");
        write("summary.xml", "<not-a-surefire-report/>");
        TestRunSummary summary = SurefireReportParser.parse(reportsDir);
        assertThat(summary.tests()).isZero();
    }

    @Test
    @DisplayName("a malformed report fails loudly instead of undercounting")
    void malformedReportFails() throws IOException {
        write("TEST-com.example.FooTest.xml", "<testsuite tests='1'");
        assertThatThrownBy(() -> SurefireReportParser.parse(reportsDir))
                .isInstanceOf(IllegalStateException.class)
                .hasMessageContaining("Unable to parse");
    }

    @Test
    @DisplayName("an unreadable reports directory surfaces as an unchecked IO error")
    void unreadableDirectoryFails() throws IOException {
        Path locked = Files.createDirectory(reportsDir.resolve("locked"));
        Files.setPosixFilePermissions(locked, PosixFilePermissions.fromString("---------"));
        try {
            assertThatThrownBy(() -> SurefireReportParser.parse(locked))
                    .isInstanceOf(UncheckedIOException.class)
                    .hasMessageContaining("Unable to read");
        } finally {
            Files.setPosixFilePermissions(locked, PosixFilePermissions.fromString("rwxr-xr-x"));
        }
    }

    private void write(String filename, String content) throws IOException {
        Files.writeString(reportsDir.resolve(filename), content.stripIndent());
    }
}
