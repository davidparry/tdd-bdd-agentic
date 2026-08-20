//! Wording-quality review of a requirement — the Rust port of the Java
//! server's `RequirementRefiner`. One rung above structural validation:
//! the validator decides whether the spec is usable, the refiner critiques
//! how well it is written. Finding strings match the Java output verbatim.

use regex::Regex;
use std::sync::LazyLock;

use crate::domain::model::Requirement;

static AMBIGUOUS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(should|could|might|handles?|properly|appropriately|quickly|easily|robust|user-friendly|etc)\b",
    )
    .expect("valid regex")
});
static WHEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bwhen\b").expect("valid regex"));
static EDGE_CASE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(empty|blank|invalid|negative|error|missing|null)\b").expect("valid regex")
});
static CONCRETE_OUTCOME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[0-9"]"#).expect("valid regex"));

/// Deterministic quality feedback so an agent (or human) can iterate:
/// reword, validate, refine, repeat — until no findings remain.
#[derive(Debug, Default)]
pub struct RequirementRefiner;

impl RequirementRefiner {
    /// Returns an empty list when the wording is clean, otherwise every finding.
    pub fn review(&self, requirement: &Requirement) -> Vec<String> {
        let mut findings = Vec::new();
        review_story(&requirement.story, &mut findings);
        for criterion in &requirement.acceptance_criteria {
            review_criterion(criterion, &mut findings);
        }
        review_coverage(&requirement.acceptance_criteria, &mut findings);
        findings
    }
}

fn review_story(story: &str, findings: &mut Vec<String>) {
    let lower = story.to_lowercase();
    if !lower.contains("as a") {
        findings.push(
            "story: missing the actor - start with 'As a ...' so we know who this is for"
                .to_string(),
        );
    }
    if !lower.contains("so that") {
        findings.push(
            "story: missing the why - finish with 'so that ...' so the value is explicit"
                .to_string(),
        );
    }
    for word in ambiguous_words(&lower) {
        findings.push(format!(
            "story: '{word}' is ambiguous - describe the observable behavior instead"
        ));
    }
}

fn review_criterion(criterion: &str, findings: &mut Vec<String>) {
    let lower = criterion.to_lowercase();
    if WHEN.find_iter(&lower).count() > 1 {
        findings.push(format!(
            "criterion \"{criterion}\": covers more than one action - split it so each \
             criterion has a single When"
        ));
    }
    if let Some(then_index) = lower.rfind("then")
        && !CONCRETE_OUTCOME.is_match(&lower[then_index..])
    {
        findings.push(format!(
            "criterion \"{criterion}\": the outcome is not concrete - state the exact \
             expected value after 'then'"
        ));
    }
    for word in ambiguous_words(&lower) {
        findings.push(format!(
            "criterion \"{criterion}\": '{word}' is ambiguous - state exactly what happens"
        ));
    }
}

fn review_coverage(criteria: &[String], findings: &mut Vec<String>) {
    if !criteria.is_empty() && !criteria.iter().any(|c| EDGE_CASE.is_match(c)) {
        findings.push(
            "criteria: only happy paths - add at least one edge case \
             (empty, invalid, or error input)"
                .to_string(),
        );
    }
}

/// A concrete "try this instead" hint for a validation or refinement
/// finding, so the person rewording a draft never has to guess what a
/// passing wording looks like. Findings without an obvious fix get none.
pub fn suggestion_for(finding: &str) -> Option<&'static str> {
    let suggestion = if finding.contains("title is missing") {
        "give the requirement a short, specific title, e.g. 'Comma-separated numbers are summed'"
    } else if finding.contains("user story is missing") || finding.contains("missing the actor") {
        "reword the story as: As a <role>, I want <capability> so that <benefit>"
    } else if finding.contains("missing the why") {
        "finish the story with 'so that <benefit>', e.g. '... so that totals come from one input.'"
    } else if finding.contains("must be phrased Given/When/Then") {
        "rephrase as: Given <starting state>, when <action>, then <exact result> - e.g. \
         Given the input \"1,2\", when add is called, then the result is 3"
    } else if finding.contains("covers more than one action") {
        "split it into two criteria, each with exactly one 'when'"
    } else if finding.contains("the outcome is not concrete") {
        "end with the exact expected value, e.g. '..., then the result is 3'"
    } else if finding.contains("is ambiguous") {
        "replace the vague word with the exact observable behavior, e.g. 'the result is 3'"
    } else if finding.contains("only happy paths") {
        "add an edge case, e.g. Given an empty string \"\", when add is called, then the result is 0"
    } else if finding.contains("at least one acceptance criterion") {
        "add at least one criterion: Given <starting state>, when <action>, then <exact result>"
    } else {
        return None;
    };
    Some(suggestion)
}

/// Distinct ambiguous words in first-seen order (the Java LinkedHashSet).
fn ambiguous_words(lower: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for capture in AMBIGUOUS.captures_iter(lower) {
        let word = capture[1].to_string();
        if !words.contains(&word) {
            words.push(word);
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(story: &str, criteria: Vec<&str>) -> Requirement {
        Requirement {
            id: "REQ-007".into(),
            title: "Newlines as delimiters".into(),
            status: "pending".into(),
            story: story.into(),
            acceptance_criteria: criteria.into_iter().map(String::from).collect(),
            feature_file: None,
        }
    }

    #[test]
    fn a_clean_requirement_has_no_findings() {
        let r = requirement(
            "As a calculator user, I want newlines to separate numbers in addition to \
             commas so that multi-line input just works.",
            vec![
                "Given the input \"1\\n2,3\", when add is called, then the result is 6",
                "Given an empty string \"\", when add is called, then the result is 0",
            ],
        );
        assert!(RequirementRefiner.review(&r).is_empty());
    }

    #[test]
    fn the_workshop_demo_story_earns_exactly_five_findings() {
        let r = requirement(
            "the calculator should handle newlines quickly",
            vec![
                "Given the input \"1\\n2,3\", when add is called, then the result is 6",
                "Given an empty string \"\", when add is called, then the result is 0",
            ],
        );
        assert_eq!(
            RequirementRefiner.review(&r),
            vec![
                "story: missing the actor - start with 'As a ...' so we know who this is for",
                "story: missing the why - finish with 'so that ...' so the value is explicit",
                "story: 'should' is ambiguous - describe the observable behavior instead",
                "story: 'handle' is ambiguous - describe the observable behavior instead",
                "story: 'quickly' is ambiguous - describe the observable behavior instead",
            ]
        );
    }

    #[test]
    fn happy_path_only_criteria_earn_the_coverage_finding() {
        let r = requirement(
            "As a user, I want newlines to separate numbers so that multi-line input works.",
            vec!["Given the input \"1\\n2,3\", when add is called, then the result is 6"],
        );
        assert_eq!(
            RequirementRefiner.review(&r),
            vec![
                "criteria: only happy paths - add at least one edge case \
                 (empty, invalid, or error input)"
            ]
        );
    }

    #[test]
    fn a_criterion_with_two_whens_is_flagged() {
        let r = requirement(
            "As a user, I want sums so that errors are visible.",
            vec!["Given a calculator, when I add and when I subtract, then 4"],
        );
        let findings = RequirementRefiner.review(&r);
        assert_eq!(
            findings,
            vec![
                "criterion \"Given a calculator, when I add and when I subtract, then 4\": \
                 covers more than one action - split it so each criterion has a single When"
                    .to_string(),
                "criteria: only happy paths - add at least one edge case \
                 (empty, invalid, or error input)"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn an_outcome_without_a_number_or_quote_is_not_concrete() {
        let r = requirement(
            "As a user, I want sums so that errors are visible.",
            vec!["Given a calculator, when I add, then it works"],
        );
        let findings = RequirementRefiner.review(&r);
        assert_eq!(
            findings,
            vec![
                "criterion \"Given a calculator, when I add, then it works\": the outcome \
                 is not concrete - state the exact expected value after 'then'"
                    .to_string(),
                "criteria: only happy paths - add at least one edge case \
                 (empty, invalid, or error input)"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn an_ambiguous_word_in_a_criterion_is_flagged() {
        // "empty" satisfies the edge-case coverage rule, so the
        // ambiguity is the only finding.
        let r = requirement(
            "As a user, I want sums so that errors are visible.",
            vec!["Given an empty string, when add is called, then it should be 0"],
        );
        assert_eq!(
            RequirementRefiner.review(&r),
            vec![
                "criterion \"Given an empty string, when add is called, then it should \
                 be 0\": 'should' is ambiguous - state exactly what happens"
            ]
        );
    }

    #[test]
    fn every_draftable_finding_gets_a_concrete_suggestion() {
        let cases = [
            ("REQ-001: title is missing", "short, specific title"),
            ("REQ-001: user story is missing", "As a <role>"),
            (
                "story: missing the actor - start with 'As a ...' so we know who this is for",
                "As a <role>",
            ),
            (
                "story: missing the why - finish with 'so that ...' so the value is explicit",
                "so that <benefit>",
            ),
            (
                "REQ-001: criterion \"x\" must be phrased Given/When/Then",
                "Given <starting state>",
            ),
            (
                "criterion \"x\": covers more than one action - split it so each \
                 criterion has a single When",
                "exactly one 'when'",
            ),
            (
                "criterion \"x\": the outcome is not concrete - state the exact \
                 expected value after 'then'",
                "exact expected value",
            ),
            (
                "story: 'should' is ambiguous - describe the observable behavior instead",
                "observable behavior",
            ),
            (
                "criteria: only happy paths - add at least one edge case \
                 (empty, invalid, or error input)",
                "empty string",
            ),
            (
                "REQ-001: at least one acceptance criterion is required",
                "add at least one criterion",
            ),
        ];
        for (finding, fragment) in cases {
            let suggestion = suggestion_for(finding)
                .unwrap_or_else(|| panic!("no suggestion for finding: {finding}"));
            assert!(
                suggestion.contains(fragment),
                "suggestion for {finding:?} was {suggestion:?}"
            );
        }
    }

    #[test]
    fn findings_without_an_obvious_fix_get_no_suggestion() {
        assert_eq!(
            suggestion_for("REQ-001: duplicate id - every requirement needs its own"),
            None
        );
    }

    #[test]
    fn ambiguous_words_are_reported_once_each_in_first_seen_order() {
        let r = requirement(
            "As a user, I want it to handle input quickly, handle it should, \
             so that errors are visible.",
            vec![],
        );
        let findings = RequirementRefiner.review(&r);
        assert_eq!(
            findings,
            vec![
                "story: 'handle' is ambiguous - describe the observable behavior instead",
                "story: 'quickly' is ambiguous - describe the observable behavior instead",
                "story: 'should' is ambiguous - describe the observable behavior instead",
            ]
        );
    }
}
