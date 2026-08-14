package com.davidparry.workshop.mcp.server.requirements;

import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Deterministic quality feedback on a requirement's wording — one rung above
 * {@link SpecValidator}. The validator decides whether the spec is
 * structurally usable; the refiner critiques how well it is written, so an
 * LLM can iterate: reword, validate, refine, repeat — until no findings
 * remain and the human signs off on the wording.
 */
public class RequirementRefiner {

    private static final Pattern AMBIGUOUS = Pattern.compile(
            "\\b(should|could|might|handles?|properly|appropriately|quickly|easily|robust|user-friendly|etc)\\b",
            Pattern.CASE_INSENSITIVE);
    private static final Pattern WHEN = Pattern.compile("\\bwhen\\b");
    private static final Pattern EDGE_CASE = Pattern.compile(
            "\\b(empty|blank|invalid|negative|error|missing|null)\\b", Pattern.CASE_INSENSITIVE);
    private static final Pattern CONCRETE_OUTCOME = Pattern.compile("[0-9\"]");

    /** Returns an empty list when the wording is clean, otherwise every finding. */
    public List<String> review(Requirement requirement) {
        List<String> findings = new ArrayList<>();
        reviewStory(requirement.story(), findings);
        for (String criterion : requirement.acceptanceCriteria()) {
            reviewCriterion(criterion, findings);
        }
        reviewCoverage(requirement.acceptanceCriteria(), findings);
        return List.copyOf(findings);
    }

    private void reviewStory(String story, List<String> findings) {
        String lower = String.valueOf(story).toLowerCase(Locale.ROOT);
        if (!lower.contains("as a")) {
            findings.add("story: missing the actor - start with 'As a ...' so we know who this is for");
        }
        if (!lower.contains("so that")) {
            findings.add("story: missing the why - finish with 'so that ...' so the value is explicit");
        }
        for (String word : ambiguousWords(lower)) {
            findings.add("story: '" + word + "' is ambiguous - describe the observable behavior instead");
        }
    }

    private void reviewCriterion(String criterion, List<String> findings) {
        String lower = criterion.toLowerCase(Locale.ROOT);
        if (WHEN.matcher(lower).results().count() > 1) {
            findings.add("criterion \"" + criterion
                    + "\": covers more than one action - split it so each criterion has a single When");
        }
        int thenIndex = lower.lastIndexOf("then");
        if (thenIndex >= 0 && !CONCRETE_OUTCOME.matcher(criterion.substring(thenIndex)).find()) {
            findings.add("criterion \"" + criterion
                    + "\": the outcome is not concrete - state the exact expected value after 'then'");
        }
        for (String word : ambiguousWords(lower)) {
            findings.add("criterion \"" + criterion + "\": '" + word + "' is ambiguous - state exactly what happens");
        }
    }

    private void reviewCoverage(List<String> criteria, List<String> findings) {
        if (!criteria.isEmpty() && criteria.stream().noneMatch(c -> EDGE_CASE.matcher(c).find())) {
            findings.add("criteria: only happy paths - add at least one edge case "
                    + "(empty, invalid, or error input)");
        }
    }

    private static Set<String> ambiguousWords(String lower) {
        Set<String> words = new LinkedHashSet<>();
        Matcher matcher = AMBIGUOUS.matcher(lower);
        while (matcher.find()) {
            words.add(matcher.group(1));
        }
        return words;
    }
}
