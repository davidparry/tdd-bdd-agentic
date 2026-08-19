package com.davidparry.workshop.mcp.server.tdd;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class MavenTestRunnerTest {

    @TempDir
    Path root;

    private static final Duration GENEROUS = Duration.ofSeconds(30);

    private MavenTestRunner runner(String script) {
        return new MavenTestRunner(root, List.of("sh", "-c", script), GENEROUS);
    }

    private void writePassingReport() throws IOException {
        Path reports = root.resolve("kata").resolve("target").resolve("surefire-reports");
        Files.createDirectories(reports);
        Files.writeString(reports.resolve("TEST-com.example.FooTest.xml"), """
                <?xml version="1.0" encoding="UTF-8"?>
                <testsuite name="com.example.FooTest" tests="2" failures="0" errors="0" skipped="0">
                  <testcase classname="com.example.FooTest" name="a" time="0.001"/>
                  <testcase classname="com.example.FooTest" name="b" time="0.001"/>
                </testsuite>
                """);
    }

    @Test
    @DisplayName("a successful run is summarized from the surefire reports")
    void successfulRunIsSummarized() throws IOException {
        writePassingReport();
        TestRunSummary summary = runner("exit 0").runKataTests();
        assertThat(summary.tests()).isEqualTo(2);
        assertThat(summary.passed()).isTrue();
    }

    @Test
    @DisplayName("no reports and a clean exit yields an empty summary")
    void noReportsCleanExit() {
        TestRunSummary summary = runner("exit 0").runKataTests();
        assertThat(summary.tests()).isZero();
        assertThat(summary.errors()).isZero();
    }

    @Test
    @DisplayName("a build failure before any test ran is reported as an error with output")
    void buildFailureIsReported() {
        TestRunSummary summary = runner("echo 'COMPILATION ERROR'; exit 1").runKataTests();
        assertThat(summary.tests()).isZero();
        assertThat(summary.errors()).isEqualTo(1);
        assertThat(summary.failureDetails().get(0)).contains("COMPILATION ERROR");
    }

    @Test
    @DisplayName("a hung build is forcibly killed after the timeout")
    void hungBuildTimesOut() {
        MavenTestRunner hung = new MavenTestRunner(
                root, List.of("sh", "-c", "exec >/dev/null 2>&1; sleep 30"), Duration.ofMillis(200));
        assertThatThrownBy(hung::runKataTests)
                .isInstanceOf(IllegalStateException.class)
                .hasMessageContaining("timed out");
    }

    @Test
    @DisplayName("an unlaunchable command surfaces as an unchecked IO error")
    void unlaunchableCommandFails() {
        MavenTestRunner broken = new MavenTestRunner(
                root, List.of(root.resolve("no-such-binary").toString()), GENEROUS);
        assertThatThrownBy(broken::runKataTests)
                .isInstanceOf(UncheckedIOException.class)
                .hasMessageContaining("Unable to launch");
    }

    @Test
    @DisplayName("an interrupted run restores the interrupt flag")
    void interruptedRunRestoresFlag() {
        MavenTestRunner slow = new MavenTestRunner(
                root, List.of("sh", "-c", "exec >/dev/null 2>&1; sleep 1"), GENEROUS);
        Thread.currentThread().interrupt();
        try {
            assertThatThrownBy(slow::runKataTests)
                    .isInstanceOf(IllegalStateException.class)
                    .hasMessageContaining("interrupted");
        } finally {
            assertThat(Thread.interrupted()).isTrue();
        }
    }

    @Test
    @DisplayName("the default command invokes Maven quietly on the standalone kata POM")
    void defaultCommandTargetsKata() {
        assertThat(MavenTestRunner.defaultCommand())
                .containsExactly(
                        MavenTestRunner.mavenExecutable(System.getProperty("os.name")),
                        "-q", "-B", "-f", "kata/pom.xml", "test");
        assertThat(new MavenTestRunner(root)).isNotNull();
    }

    @Test
    @DisplayName("the Maven executable is mvn.cmd on Windows and mvn elsewhere")
    void mavenExecutablePerOs() {
        assertThat(MavenTestRunner.mavenExecutable("Windows 11")).isEqualTo("mvn.cmd");
        assertThat(MavenTestRunner.mavenExecutable("Mac OS X")).isEqualTo("mvn");
        assertThat(MavenTestRunner.mavenExecutable("Linux")).isEqualTo("mvn");
    }

    @Test
    @DisplayName("tail keeps only the last N lines of long output")
    void tailKeepsLastLines() {
        String fifty = String.join("\n",
                java.util.stream.IntStream.rangeClosed(1, 50).mapToObj(i -> "line" + i).toList());
        String tail = MavenTestRunner.tail(fifty, 30);
        assertThat(tail).startsWith("line21").endsWith("line50").doesNotContain("line20\n");
        assertThat(MavenTestRunner.tail("short", 30)).isEqualTo("short");
    }
}
