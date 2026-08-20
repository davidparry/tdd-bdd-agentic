package com.davidparry.workshop.mcp.server.requirements;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

class RequirementsRepositoryTest {

    @TempDir
    Path dir;

    private RequirementsRepository repository;

    @BeforeEach
    void setUp() throws IOException {
        Path file = dir.resolve("requirements.json");
        Files.writeString(file, """
                {
                  "project": "Test Kata",
                  "description": "fixture",
                  "requirements": [
                    {
                      "id": "REQ-001",
                      "title": "First",
                      "status": "implemented",
                      "story": "As a user...",
                      "acceptanceCriteria": ["Given x, then y"]
                    },
                    {
                      "id": "REQ-002",
                      "title": "Second",
                      "status": "pending",
                      "story": "As a user...",
                      "acceptanceCriteria": ["Given a, then b", "Given c, then d"],
                      "featureFile": "kata/src/test/resources/features/string_calculator.feature"
                    }
                  ]
                }
                """);
        repository = RequirementsRepository.load(file);
    }

    @Test
    @DisplayName("loads every requirement from the json file")
    void loadsAllRequirements() {
        assertThat(repository.projectName()).isEqualTo("Test Kata");
        assertThat(repository.all()).hasSize(2);
    }

    @Test
    @DisplayName("finds a requirement by id, case-insensitively")
    void findsById() {
        assertThat(repository.byId("req-002")).isPresent();
        assertThat(repository.byId("REQ-002").orElseThrow().acceptanceCriteria()).hasSize(2);
        assertThat(repository.byId("REQ-999")).isEmpty();
    }

    @Test
    @DisplayName("featureFile is optional and loads when present")
    void featureFileIsOptional() {
        assertThat(repository.byId("REQ-001").orElseThrow().featureFile()).isNull();
        assertThat(repository.byId("REQ-002").orElseThrow().featureFile())
                .isEqualTo("kata/src/test/resources/features/string_calculator.feature");
    }

    @Test
    @DisplayName("nextPending returns the first requirement not yet implemented")
    void nextPendingSkipsImplemented() {
        assertThat(repository.nextPending().orElseThrow().id()).isEqualTo("REQ-002");
    }

    @Test
    @DisplayName("nextPending is empty when every requirement is implemented")
    void nextPendingEmptyWhenAllImplemented() throws IOException {
        Path file = dir.resolve("done.json");
        Files.writeString(file, """
                {
                  "project": "Done Kata",
                  "requirements": [
                    {
                      "id": "REQ-001",
                      "title": "First",
                      "status": "implemented",
                      "story": "As a user...",
                      "acceptanceCriteria": ["Given x, then y"]
                    }
                  ]
                }
                """);
        assertThat(RequirementsRepository.load(file).nextPending()).isEmpty();
    }

    @Test
    @DisplayName("includes merge depth-first, a file's own requirements before its includes'")
    void includesMergeDepthFirst() throws IOException {
        Files.createDirectories(dir.resolve("core"));
        Files.writeString(dir.resolve("root.json"), """
                {
                  "project": "Catalog Kata",
                  "includes": ["core/math.json"],
                  "requirements": [
                    {"id": "REQ-001", "title": "Root", "status": "pending",
                     "story": "s", "acceptanceCriteria": ["Given x, then y"]}
                  ]
                }
                """);
        // ".//extra.json" exercises the "." and empty path segments;
        // "../side.json" walks back up to the root directory.
        Files.writeString(dir.resolve("core/math.json"), """
                {
                  "includes": [".//extra.json", "../side.json"],
                  "requirements": [
                    {"id": "REQ-002", "title": "Math", "status": "pending",
                     "story": "s", "acceptanceCriteria": ["Given x, then y"]}
                  ]
                }
                """);
        Files.writeString(dir.resolve("core/extra.json"), """
                {
                  "requirements": [
                    {"id": "REQ-003", "title": "Extra", "status": "pending",
                     "story": "s", "acceptanceCriteria": ["Given x, then y"]}
                  ]
                }
                """);
        // A pure catalog file: no requirements of its own.
        Files.writeString(dir.resolve("side.json"), """
                {"includes": []}
                """);
        RequirementsRepository catalog = RequirementsRepository.load(dir.resolve("root.json"));
        assertThat(catalog.projectName()).isEqualTo("Catalog Kata");
        assertThat(catalog.all()).extracting(Requirement::id)
                .containsExactly("REQ-001", "REQ-002", "REQ-003");
        assertThat(catalog.allWithSources())
                .extracting(RequirementsRepository.SourcedRequirement::path)
                .containsExactly("root.json", "core/math.json", "core/extra.json");
        assertThat(catalog.byId("REQ-003")).isPresent();
    }

    @Test
    @DisplayName("an include cycle throws a SpecException naming the repeated file")
    void includeCycleThrows() throws IOException {
        Files.writeString(dir.resolve("a.json"), """
                {"project": "Kata", "includes": ["b.json"], "requirements": []}
                """);
        Files.writeString(dir.resolve("b.json"), """
                {"includes": ["a.json"], "requirements": []}
                """);
        assertThatThrownBy(() -> RequirementsRepository.load(dir.resolve("a.json")))
                .isInstanceOf(SpecException.class)
                .hasMessage("spec: a.json is included more than once - include every "
                        + "spec file exactly once");
    }

    @Test
    @DisplayName("a missing included file throws a SpecException naming the child")
    void missingIncludeThrows() throws IOException {
        Files.writeString(dir.resolve("a.json"), """
                {"project": "Kata", "includes": ["missing.json"], "requirements": []}
                """);
        assertThatThrownBy(() -> RequirementsRepository.load(dir.resolve("a.json")))
                .isInstanceOf(SpecException.class)
                .hasMessageStartingWith("spec: missing.json is not readable JSON -");
    }

    @Test
    @DisplayName("an include escaping the spec directory throws a SpecException")
    void escapingIncludeThrows() throws IOException {
        Files.writeString(dir.resolve("a.json"), """
                {"project": "Kata", "includes": ["../outside.json"], "requirements": []}
                """);
        assertThatThrownBy(() -> RequirementsRepository.load(dir.resolve("a.json")))
                .isInstanceOf(SpecException.class)
                .hasMessage("spec: include \"../outside.json\" in a.json escapes the spec directory");
        Files.writeString(dir.resolve("a.json"), """
                {"project": "Kata", "includes": ["."], "requirements": []}
                """);
        assertThatThrownBy(() -> RequirementsRepository.load(dir.resolve("a.json")))
                .isInstanceOf(SpecException.class)
                .hasMessage("spec: include \".\" in a.json escapes the spec directory");
    }

    @Test
    @DisplayName("a bare relative root path loads against the working directory")
    void bareRootPathReportsReadableError() {
        assertThatThrownBy(() -> RequirementsRepository.load(Path.of("no-such-spec.json")))
                .isInstanceOf(SpecException.class)
                .hasMessageStartingWith("spec: no-such-spec.json is not readable JSON -");
    }

    @Test
    @DisplayName("a root path without a file name still reports a readable error")
    void rootWithoutFileNameReportsReadableError() {
        assertThatThrownBy(() -> RequirementsRepository.load(Path.of("/")))
                .isInstanceOf(SpecException.class)
                .hasMessageStartingWith("spec: / is not readable JSON -");
    }

    @Test
    @DisplayName("missing acceptanceCriteria loads as an empty immutable list")
    void missingAcceptanceCriteriaIsEmptyList() throws IOException {
        Path file = dir.resolve("sparse.json");
        Files.writeString(file, """
                {
                  "project": "Sparse Kata",
                  "requirements": [
                    {
                      "id": "REQ-001",
                      "title": "First",
                      "status": "pending",
                      "story": "As a user..."
                    }
                  ]
                }
                """);
        assertThat(RequirementsRepository.load(file).byId("REQ-001").orElseThrow().acceptanceCriteria())
                .isEmpty();
    }
}
