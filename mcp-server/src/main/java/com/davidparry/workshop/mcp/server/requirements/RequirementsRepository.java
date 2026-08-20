package com.davidparry.workshop.mcp.server.requirements;

import com.fasterxml.jackson.annotation.JsonIgnoreProperties;

import tools.jackson.databind.ObjectMapper;

import java.nio.file.Path;
import java.util.List;
import java.util.Optional;

/**
 * Loads workshop requirements from {@code requirements/requirements.json}.
 */
public class RequirementsRepository {

    @JsonIgnoreProperties(ignoreUnknown = true)
    record RequirementsFile(String project, List<Requirement> requirements) {
    }

    private final RequirementsFile file;

    private RequirementsRepository(RequirementsFile file) {
        this.file = file;
    }

    /** Loads the spec; Jackson 3 (tools.jackson) throws unchecked exceptions on failure. */
    public static RequirementsRepository load(Path requirementsJson) {
        return new RequirementsRepository(
                new ObjectMapper().readValue(requirementsJson.toFile(), RequirementsFile.class));
    }

    public String projectName() {
        return file.project();
    }

    public List<Requirement> all() {
        return file.requirements();
    }

    public Optional<Requirement> byId(String id) {
        return file.requirements().stream()
                .filter(r -> r.id().equalsIgnoreCase(id))
                .findFirst();
    }

    public Optional<Requirement> nextPending() {
        return file.requirements().stream()
                .filter(Requirement::isPending)
                .findFirst();
    }
}
