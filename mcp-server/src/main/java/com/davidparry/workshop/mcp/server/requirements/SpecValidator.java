package com.davidparry.workshop.mcp.server.requirements;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * Validates the requirements spec on disk so an agent can iterate on it:
 * draft or edit a requirement, call {@code validate_spec}, fix the reported
 * issues, and repeat until the spec is valid. Reads the file fresh on every
 * call — the spec the agent just wrote is the spec that gets validated.
 */
public class SpecValidator {

    private static final Pattern ID = Pattern.compile("[A-Z][A-Z0-9]*-\\d+");
    private static final Set<String> STATUSES = Set.of("pending", "implemented");

    private final Path specFile;
    private final Path repoRoot;

    public SpecValidator(Path specFile, Path repoRoot) {
        this.specFile = specFile;
        this.repoRoot = repoRoot;
    }

    /** Returns an empty list when the spec is valid, otherwise every issue found. */
    public List<String> validate() {
        List<Requirement> requirements;
        List<String> issues = new ArrayList<>();
        try {
            RequirementsRepository repo = RequirementsRepository.load(specFile);
            if (isBlank(repo.projectName())) {
                issues.add("spec: the project name is missing");
            }
            requirements = repo.all();
        } catch (RuntimeException e) {
            return List.of("spec: " + specFile.getFileName() + " is not readable JSON - " + e.getMessage());
        }
        if (requirements == null || requirements.isEmpty()) {
            issues.add("spec: the requirements array is missing or empty");
            return List.copyOf(issues);
        }
        Set<String> seenIds = new HashSet<>();
        for (Requirement r : requirements) {
            validateRequirement(r, seenIds, issues);
        }
        return List.copyOf(issues);
    }

    private void validateRequirement(Requirement r, Set<String> seenIds, List<String> issues) {
        String id = String.valueOf(r.id());
        if (!ID.matcher(id).matches()) {
            issues.add(id + ": id must look like REQ-007 (uppercase prefix, dash, number)");
        }
        if (!seenIds.add(id)) {
            issues.add(id + ": duplicate id - every requirement needs its own");
        }
        if (isBlank(r.title())) {
            issues.add(id + ": title is missing");
        }
        if (isBlank(r.story())) {
            issues.add(id + ": user story is missing");
        }
        if (r.acceptanceCriteria().isEmpty()) {
            issues.add(id + ": at least one acceptance criterion is required");
        }
        for (String criterion : r.acceptanceCriteria()) {
            String lower = criterion.toLowerCase(Locale.ROOT);
            if (!lower.contains("given") || !lower.contains("when") || !lower.contains("then")) {
                issues.add(id + ": criterion \"" + criterion + "\" must be phrased Given/When/Then");
            }
        }
        if (!STATUSES.contains(String.valueOf(r.status()).toLowerCase(Locale.ROOT))) {
            issues.add(id + ": status must be 'pending' or 'implemented'");
        }
        validateFeatureFile(r, id, issues);
    }

    private void validateFeatureFile(Requirement r, String id, List<String> issues) {
        if (isBlank(r.featureFile())) {
            if (!r.isPending()) {
                issues.add(id + ": implemented requirements must name their featureFile");
            }
            return;
        }
        Path feature = repoRoot.resolve(r.featureFile());
        if (!Files.isRegularFile(feature)) {
            issues.add(id + ": featureFile " + r.featureFile() + " does not exist");
            return;
        }
        if (!r.isPending() && !hasTag(feature, id)) {
            issues.add(id + ": no scenario tagged @" + id + " in " + r.featureFile()
                    + " - implemented requirements need executable scenarios");
        }
    }

    private static boolean hasTag(Path feature, String id) {
        try {
            return Files.readString(feature).contains("@" + id);
        } catch (IOException e) {
            return false;
        }
    }

    private static boolean isBlank(String value) {
        return value == null || value.isBlank();
    }
}
