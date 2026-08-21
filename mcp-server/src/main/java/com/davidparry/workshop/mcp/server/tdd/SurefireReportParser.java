package com.davidparry.workshop.mcp.server.tdd;

import javax.xml.parsers.DocumentBuilderFactory;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Stream;

import org.w3c.dom.Document;
import org.w3c.dom.Element;
import org.w3c.dom.NodeList;

/**
 * Parses Maven Surefire XML reports ({@code TEST-*.xml}) into a
 * {@link TestRunSummary}.
 */
public final class SurefireReportParser {

    private SurefireReportParser() {
    }

    public static TestRunSummary parse(Path surefireReportsDir) {
        if (!Files.isDirectory(surefireReportsDir)) {
            return TestRunSummary.empty();
        }
        int tests = 0;
        int failures = 0;
        int errors = 0;
        int skipped = 0;
        List<String> details = new ArrayList<>();

        try (Stream<Path> files = Files.list(surefireReportsDir)) {
            List<Path> reports = files
                    .filter(p -> fileName(p).startsWith("TEST-"))
                    .filter(p -> fileName(p).endsWith(".xml"))
                    .sorted()
                    .toList();
            for (Path report : reports) {
                Document doc = DocumentBuilderFactory.newInstance()
                        .newDocumentBuilder()
                        .parse(report.toFile());
                Element suite = doc.getDocumentElement();
                tests += intAttr(suite, "tests");
                failures += intAttr(suite, "failures");
                errors += intAttr(suite, "errors");
                skipped += intAttr(suite, "skipped");
                details.addAll(failureDetails(suite));
            }
        } catch (IOException e) {
            throw new java.io.UncheckedIOException("Unable to read surefire reports in " + surefireReportsDir, e);
        } catch (Exception e) {
            throw new IllegalStateException("Unable to parse surefire reports in " + surefireReportsDir, e);
        }
        return new TestRunSummary(tests, failures, errors, skipped, List.copyOf(details));
    }

    private static String fileName(Path path) {
        return java.util.Objects.toString(path.getFileName(), "");
    }

    private static int intAttr(Element element, String name) {
        String value = element.getAttribute(name);
        return value.isBlank() ? 0 : Integer.parseInt(value);
    }

    private static List<String> failureDetails(Element suite) {
        List<String> details = new ArrayList<>();
        NodeList testcases = suite.getElementsByTagName("testcase");
        for (int i = 0; i < testcases.getLength(); i++) {
            Element testcase = (Element) testcases.item(i);
            for (String tag : List.of("failure", "error")) {
                NodeList problems = testcase.getElementsByTagName(tag);
                for (int j = 0; j < problems.getLength(); j++) {
                    Element problem = (Element) problems.item(j);
                    String message = problem.getAttribute("message");
                    details.add(testcase.getAttribute("classname") + "." + testcase.getAttribute("name")
                            + ": " + (message.isBlank() ? tag : message));
                }
            }
        }
        return details;
    }
}
