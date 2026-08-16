package com.davidparry.workshop.mcp.server.requirements;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.PosixFilePermissions;
import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;

class SpecValidatorTest {

    @TempDir
    Path root;

    private SpecValidator validator;

    private List<String> validate(String json) throws IOException {
        Path spec = root.resolve("requirements.json");
        Files.writeString(spec, json);
        validator = new SpecValidator(spec, root);
        return validator.validate();
    }

    private void writeFeature(String relativePath, String content) throws IOException {
        Path feature = root.resolve(relativePath);
        Files.createDirectories(feature.getParent());
        Files.writeString(feature, content);
    }

    private static String requirement(String id, String title, String story,
                                      String criteria, String status, String featureFile) {
        return """
                {
                  "id": %s,
                  "title": %s,
                  "story": %s,
                  "acceptanceCriteria": %s,
                  "status": %s%s
                }""".formatted(id, title, story, criteria, status,
                featureFile == null ? "" : ",\n  \"featureFile\": " + featureFile);
    }

    private static String spec(String project, String... requirements) {
        return """
                {
                  "project": %s,
                  "requirements": [%s]
                }""".formatted(project, String.join(",\n", requirements));
    }

    @Test
    @DisplayName("a complete, well-formed spec is valid")
    void completeSpecIsValid() throws IOException {
        writeFeature("features/calc.feature", "@REQ-001\nScenario: done");
        List<String> issues = validate(spec("\"Kata\"",
                requirement("\"REQ-001\"", "\"First\"", "\"As a user...\"",
                        "[\"Given x, when y, then z\"]", "\"implemented\"", "\"features/calc.feature\""),
                requirement("\"REQ-002\"", "\"Second\"", "\"As a user...\"",
                        "[\"Given a, when b, then c\"]", "\"pending\"", null)));
        assertThat(issues).isEmpty();
    }

    @Test
    @DisplayName("unreadable JSON is one actionable issue, not a crash")
    void unreadableJsonIsReported() throws IOException {
        List<String> issues = validate("{ not json");
        assertThat(issues).hasSize(1);
        assertThat(issues.get(0)).contains("not readable JSON");
    }

    @Test
    @DisplayName("a missing project name is reported")
    void missingProjectName() throws IOException {
        List<String> issues = validate(spec("null",
                requirement("\"REQ-001\"", "\"First\"", "\"Story\"",
                        "[\"Given x, when y, then z\"]", "\"pending\"", null)));
        assertThat(issues).contains("spec: the project name is missing");
    }

    @Test
    @DisplayName("a missing or empty requirements array is reported")
    void missingRequirements() throws IOException {
        assertThat(validate("{\"project\": \"Kata\"}"))
                .contains("spec: the requirements array is missing or empty");
        assertThat(validate(spec("\"Kata\"")))
                .contains("spec: the requirements array is missing or empty");
    }

    @Test
    @DisplayName("malformed and duplicate ids are reported")
    void badIds() throws IOException {
        List<String> issues = validate(spec("\"Kata\"",
                requirement("\"req 1\"", "\"A\"", "\"S\"", "[\"Given g, when w, then t\"]", "\"pending\"", null),
                requirement("null", "\"B\"", "\"S\"", "[\"Given g, when w, then t\"]", "\"pending\"", null),
                requirement("\"REQ-001\"", "\"C\"", "\"S\"", "[\"Given g, when w, then t\"]", "\"pending\"", null),
                requirement("\"REQ-001\"", "\"D\"", "\"S\"", "[\"Given g, when w, then t\"]", "\"pending\"", null)));
        assertThat(issues)
                .anyMatch(i -> i.startsWith("req 1: id must look like"))
                .anyMatch(i -> i.startsWith("null: id must look like"))
                .anyMatch(i -> i.contains("REQ-001: duplicate id"));
    }

    @Test
    @DisplayName("missing titles and stories are reported")
    void missingTitleAndStory() throws IOException {
        List<String> issues = validate(spec("\"Kata\"",
                requirement("\"REQ-001\"", "null", "\" \"", "[\"Given g, when w, then t\"]", "\"pending\"", null)));
        assertThat(issues)
                .contains("REQ-001: title is missing")
                .contains("REQ-001: user story is missing");
    }

    @Test
    @DisplayName("acceptance criteria must exist and be phrased Given/When/Then")
    void criteriaShapeIsChecked() throws IOException {
        List<String> issues = validate(spec("\"Kata\"",
                requirement("\"REQ-001\"", "\"A\"", "\"S\"", "[]", "\"pending\"", null),
                requirement("\"REQ-002\"", "\"B\"", "\"S\"",
                        "[\"when w, then t\", \"Given g, then t\", \"Given g, when w\", \"Given g, when w, then t\"]",
                        "\"pending\"", null)));
        assertThat(issues)
                .contains("REQ-001: at least one acceptance criterion is required")
                .anyMatch(i -> i.contains("\"when w, then t\" must be phrased"))
                .anyMatch(i -> i.contains("\"Given g, then t\" must be phrased"))
                .anyMatch(i -> i.contains("\"Given g, when w\" must be phrased"));
        assertThat(issues).noneMatch(i -> i.contains("\"Given g, when w, then t\""));
    }

    @Test
    @DisplayName("an unknown status is reported")
    void unknownStatus() throws IOException {
        List<String> issues = validate(spec("\"Kata\"",
                requirement("\"REQ-001\"", "\"A\"", "\"S\"", "[\"Given g, when w, then t\"]", "\"done\"", null)));
        assertThat(issues).contains("REQ-001: status must be 'pending' or 'implemented'");
    }

    @Test
    @DisplayName("implemented requirements must name an existing, tagged feature file")
    void implementedRequirementsNeedTaggedFeature() throws IOException {
        writeFeature("features/untagged.feature", "Scenario: no tags here");
        List<String> issues = validate(spec("\"Kata\"",
                requirement("\"REQ-001\"", "\"A\"", "\"S\"",
                        "[\"Given g, when w, then t\"]", "\"implemented\"", null),
                requirement("\"REQ-002\"", "\"B\"", "\"S\"",
                        "[\"Given g, when w, then t\"]", "\"implemented\"", "\"features/missing.feature\""),
                requirement("\"REQ-003\"", "\"C\"", "\"S\"",
                        "[\"Given g, when w, then t\"]", "\"implemented\"", "\"features/untagged.feature\"")));
        assertThat(issues)
                .contains("REQ-001: implemented requirements must name their featureFile")
                .anyMatch(i -> i.contains("REQ-002: featureFile features/missing.feature does not exist"))
                .anyMatch(i -> i.contains("REQ-003: no scenario tagged @REQ-003"));
    }

    @Test
    @DisplayName("a pending requirement may name a feature file that has no scenario yet")
    void pendingRequirementFeatureFileNeedsNoTag() throws IOException {
        writeFeature("features/untagged.feature", "Scenario: no tags here");
        List<String> issues = validate(spec("\"Kata\"",
                requirement("\"REQ-001\"", "\"A\"", "\"S\"",
                        "[\"Given g, when w, then t\"]", "\"pending\"", "\"features/untagged.feature\"")));
        assertThat(issues).isEmpty();
    }

    @Test
    @DisplayName("an unreadable feature file counts as missing the tag")
    void unreadableFeatureFileFailsTheTagCheck() throws IOException {
        writeFeature("features/locked.feature", "@REQ-001\nScenario: done");
        Path locked = root.resolve("features/locked.feature");
        Files.setPosixFilePermissions(locked, PosixFilePermissions.fromString("---------"));
        try {
            List<String> issues = validate(spec("\"Kata\"",
                    requirement("\"REQ-001\"", "\"A\"", "\"S\"",
                            "[\"Given g, when w, then t\"]", "\"implemented\"", "\"features/locked.feature\"")));
            assertThat(issues).anyMatch(i -> i.contains("REQ-001: no scenario tagged @REQ-001"));
        } finally {
            Files.setPosixFilePermissions(locked, PosixFilePermissions.fromString("rw-r--r--"));
        }
    }
}
