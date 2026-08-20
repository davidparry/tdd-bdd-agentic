package com.davidparry.workshop.mcp.server.requirements;

import com.fasterxml.jackson.annotation.JsonIgnoreProperties;

import java.util.List;

/**
 * A single workshop requirement whose acceptance criteria an agent turns
 * into executable tests.
 */
@JsonIgnoreProperties(ignoreUnknown = true)
public record Requirement(
        String id,
        String title,
        String status,
        String story,
        List<String> acceptanceCriteria,
        String featureFile) {

    public Requirement {
        acceptanceCriteria = acceptanceCriteria == null ? List.of() : List.copyOf(acceptanceCriteria);
    }

    public boolean isPending() {
        return "pending".equalsIgnoreCase(status);
    }
}
