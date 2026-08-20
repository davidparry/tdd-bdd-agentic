package com.davidparry.workshop.mcp.server.tdd;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.time.Duration;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.TimeUnit;

/**
 * Runs the standalone kata project's test suite with Maven and summarizes
 * the results from the Surefire XML reports under {@code kata/target}.
 */
public class MavenTestRunner implements TestRunner {

    private static final Duration DEFAULT_TIMEOUT = Duration.ofMinutes(5);

    private final Path workshopRoot;
    private final List<String> command;
    private final Duration timeout;

    public MavenTestRunner(Path workshopRoot) {
        this(workshopRoot, defaultCommand(), DEFAULT_TIMEOUT);
    }

    /** Visible for tests: run an arbitrary command in place of Maven. */
    MavenTestRunner(Path workshopRoot, List<String> command, Duration timeout) {
        this.workshopRoot = workshopRoot;
        this.command = List.copyOf(command);
        this.timeout = timeout;
    }

    @Override
    public TestRunSummary runKataTests() {
        try {
            Process process = new ProcessBuilder(command)
                    .directory(workshopRoot.toFile())
                    .redirectErrorStream(true)
                    .start();
            String output = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
            boolean finished = process.waitFor(timeout.toMillis(), TimeUnit.MILLISECONDS);
            if (!finished) {
                process.destroyForcibly();
                throw new IllegalStateException("Maven test run timed out after " + timeout.toSeconds() + " seconds");
            }
            TestRunSummary summary = SurefireReportParser.parse(
                    workshopRoot.resolve("kata").resolve("target").resolve("surefire-reports"));
            // A compile error produces no surefire reports but a non-zero exit code.
            if (summary.tests() == 0 && process.exitValue() != 0) {
                return new TestRunSummary(0, 0, 1, 0,
                        List.of("Build failed before tests could run:\n" + tail(output, 30)));
            }
            return summary;
        } catch (IOException e) {
            throw new UncheckedIOException("Unable to launch Maven in " + workshopRoot, e);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("Maven test run was interrupted", e);
        }
    }

    static List<String> defaultCommand() {
        return List.of(
                mavenExecutable(System.getProperty("os.name")),
                "-q",
                "-B",
                "-f", "kata/pom.xml",
                "test");
    }

    static String mavenExecutable(String osName) {
        return osName.toLowerCase(Locale.ROOT).contains("win") ? "mvn.cmd" : "mvn";
    }

    static String tail(String text, int lines) {
        List<String> all = text.lines().toList();
        int from = Math.max(0, all.size() - lines);
        return String.join("\n", all.subList(from, all.size()));
    }
}
