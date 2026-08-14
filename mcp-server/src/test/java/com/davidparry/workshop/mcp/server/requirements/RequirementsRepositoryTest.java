package com.davidparry.workshop.mcp.server.requirements;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.assertj.core.api.Assertions.assertThat;

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
