package com.davidparry.workshop.mcp.server.requirements;

import com.fasterxml.jackson.annotation.JsonIgnoreProperties;

import tools.jackson.databind.ObjectMapper;

import java.nio.file.Path;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.HashSet;
import java.util.List;
import java.util.Optional;
import java.util.Set;

/**
 * Loads the workshop requirements catalog from
 * {@code requirements/requirements.json}. The root document is always the
 * entry point; it holds requirements of its own and may include child
 * spec files, which may include further files, N levels deep. Every
 * include resolves relative to the file declaring it, and the whole tree
 * merges into one backlog depth-first — a file's own requirements before
 * its includes'.
 */
public class RequirementsRepository {

    @JsonIgnoreProperties(ignoreUnknown = true)
    record RequirementsFile(String project, List<String> includes, List<Requirement> requirements) {
    }

    /**
     * One merged requirement plus the catalog file declaring it (path
     * relative to the root document's directory).
     */
    public record SourcedRequirement(String path, Requirement requirement) {
    }

    private final String project;
    private final List<SourcedRequirement> requirements;

    private RequirementsRepository(String project, List<SourcedRequirement> requirements) {
        this.project = project;
        this.requirements = requirements;
    }

    /**
     * Loads and merges the spec tree. Resolution failures (missing or
     * unparseable files, include cycles, escaping includes) throw
     * {@link SpecException} with an already formatted message.
     */
    public static RequirementsRepository load(Path requirementsJson) {
        Path parent = requirementsJson.getParent();
        Path dir = parent == null ? Path.of("") : parent;
        Path fileName = requirementsJson.getFileName();
        String rootName = fileName == null ? requirementsJson.toString() : fileName.toString();
        List<SourcedRequirement> merged = new ArrayList<>();
        RequirementsFile root = walk(dir, rootName, new HashSet<>(), merged);
        return new RequirementsRepository(root.project(), List.copyOf(merged));
    }

    private static RequirementsFile walk(Path rootDir, String path, Set<String> visited,
                                         List<SourcedRequirement> merged) {
        if (!visited.add(path)) {
            throw new SpecException("spec: " + path
                    + " is included more than once - include every spec file exactly once");
        }
        RequirementsFile document;
        try {
            document = new ObjectMapper().readValue(rootDir.resolve(path).toFile(), RequirementsFile.class);
        } catch (RuntimeException e) {
            throw new SpecException("spec: " + path + " is not readable JSON - " + e.getMessage());
        }
        for (Requirement requirement : document.requirements() == null
                ? List.<Requirement>of()
                : document.requirements()) {
            merged.add(new SourcedRequirement(path, requirement));
        }
        String dir = parentOf(path);
        for (String include : document.includes() == null ? List.<String>of() : document.includes()) {
            String child = resolveInclude(dir, include);
            if (child == null) {
                throw new SpecException("spec: include \"" + include + "\" in " + path
                        + " escapes the spec directory");
            }
            walk(rootDir, child, visited, merged);
        }
        return document;
    }

    private static String parentOf(String path) {
        int cut = path.lastIndexOf('/');
        return cut < 0 ? "" : path.substring(0, cut);
    }

    /**
     * Resolves an include against the declaring file's directory,
     * lexically: {@code .} stays put, {@code ..} pops. Null when the
     * path escapes above the root document's directory.
     */
    private static String resolveInclude(String dir, String include) {
        Deque<String> parts = new ArrayDeque<>();
        if (!dir.isEmpty()) {
            for (String part : dir.split("/")) {
                parts.addLast(part);
            }
        }
        for (String part : include.split("/")) {
            switch (part) {
                case "", "." -> {
                }
                case ".." -> {
                    if (parts.pollLast() == null) {
                        return null;
                    }
                }
                default -> parts.addLast(part);
            }
        }
        return parts.isEmpty() ? null : String.join("/", parts);
    }

    public String projectName() {
        return project;
    }

    public List<Requirement> all() {
        return requirements.stream().map(SourcedRequirement::requirement).toList();
    }

    /** Every requirement plus the catalog file declaring it, in merged order. */
    public List<SourcedRequirement> allWithSources() {
        return List.copyOf(requirements);
    }

    public Optional<Requirement> byId(String id) {
        return requirements.stream()
                .map(SourcedRequirement::requirement)
                .filter(r -> r.id().equalsIgnoreCase(id))
                .findFirst();
    }

    public Optional<Requirement> nextPending() {
        return requirements.stream()
                .map(SourcedRequirement::requirement)
                .filter(Requirement::isPending)
                .findFirst();
    }
}
