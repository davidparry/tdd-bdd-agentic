//! Splitting a plain-words description into proposed requirements.
//! The model decides how many distinct requirements the description
//! contains; a proposal only qualifies when it arrives complete -
//! title, story, and at least one acceptance criterion.

use serde::{Deserialize, Serialize};

use crate::domain::generation::strip_code_fences;
use crate::domain::model::Requirement;
use crate::domain::prompts::{RenderedPrompt, render};
use crate::domain::refiner::suggestion_for;

/// One requirement the model proposes from the description.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProposedRequirement {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub story: String,
    #[serde(default, alias = "acceptanceCriteria")]
    pub acceptance_criteria: Vec<String>,
}

/// The strict instructions for turning a description into a JSON array
/// of complete requirements, rendered from the `[proposal]` templates.
pub fn proposal_prompt(description: &str) -> RenderedPrompt {
    render("proposal", minijinja::context! { description })
}

/// Parse the model's reply into qualified proposals. Incomplete
/// elements are dropped; an unparseable reply is an empty list - the
/// caller falls back to manual drafting either way.
pub fn parse_proposals(reply: &str) -> Vec<ProposedRequirement> {
    let body = strip_code_fences(reply);
    let Ok(proposals) = serde_json::from_str::<Vec<ProposedRequirement>>(&body) else {
        return Vec::new();
    };
    proposals
        .into_iter()
        .filter(|proposal| {
            !proposal.title.trim().is_empty()
                && !proposal.story.trim().is_empty()
                && !proposal.acceptance_criteria.is_empty()
                && proposal
                    .acceptance_criteria
                    .iter()
                    .all(|criterion| !criterion.trim().is_empty())
        })
        .collect()
}

/// One earlier wording of the draft and the findings it produced, as
/// the `[rewording]` user template consumes it.
#[derive(Serialize)]
struct WordingContext<'a> {
    title: &'a str,
    story: &'a str,
    criteria: &'a [String],
    findings: &'a [String],
}

/// The strict instructions for rewording one draft from a single
/// validate + refine finding: same requirement, same meaning, exactly
/// this one finding addressed. The caller chains one call per finding,
/// each briefed with the draft the previous call produced. `history`
/// recounts every earlier wording of this draft and the findings it
/// produced, so the model never circles back to a wording the review
/// already rejected. Rendered from the `[rewording]` templates.
pub fn rewording_prompt(
    candidate: &Requirement,
    finding: &str,
    history: &[(Requirement, Vec<String>)],
) -> RenderedPrompt {
    let history: Vec<WordingContext> = history
        .iter()
        .map(|(wording, findings)| WordingContext {
            title: &wording.title,
            story: &wording.story,
            criteria: &wording.acceptance_criteria,
            findings,
        })
        .collect();
    render(
        "rewording",
        minijinja::context! {
            title => candidate.title,
            story => candidate.story,
            criteria => candidate.acceptance_criteria,
            finding,
            hint => suggestion_for(finding),
            history,
        },
    )
}

/// Parse the model's rewording reply. `None` unless the object arrives
/// complete - the caller falls back to hand rewording.
pub fn parse_rewording(reply: &str) -> Option<ProposedRequirement> {
    let body = strip_code_fences(reply);
    let proposal = serde_json::from_str::<ProposedRequirement>(&body).ok()?;
    let complete = !proposal.title.trim().is_empty()
        && !proposal.story.trim().is_empty()
        && !proposal.acceptance_criteria.is_empty()
        && proposal
            .acceptance_criteria
            .iter()
            .all(|criterion| !criterion.trim().is_empty());
    complete.then_some(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> Requirement {
        Requirement {
            id: "REQ-001".into(),
            title: "Convert string to number".into(),
            status: "pending".into(),
            story: "As a user, I want conversion so that input becomes numbers.".into(),
            acceptance_criteria: vec![
                "Given \"1,2\", when add is called, then the result is 3".into(),
            ],
            feature_file: None,
        }
    }

    #[test]
    fn the_rewording_prompt_carries_the_draft_one_finding_its_hint_and_rules() {
        let prompt = rewording_prompt(
            &candidate(),
            "criteria: only happy paths - add at least one edge case",
            &[],
        );
        assert!(prompt.user.contains("title: Convert string to number"));
        assert!(
            prompt
                .user
                .contains("- Given \"1,2\", when add is called, then the result is 3")
        );
        assert!(prompt.user.contains("The one review finding to address:"));
        assert!(prompt.user.contains("- criteria: only happy paths"));
        assert!(prompt.user.contains("hint: add an edge case"));
        assert!(prompt.system.contains("ONLY one JSON object"));
        assert!(prompt.system.contains("Address only this one finding"));
        assert!(
            !prompt.user.contains("Earlier wordings"),
            "a first pass has no history section"
        );
    }

    #[test]
    fn a_finding_without_a_hint_is_listed_plain() {
        let prompt = rewording_prompt(&candidate(), "something entirely novel", &[]);
        assert!(prompt.user.contains("- something entirely novel"));
        assert!(!prompt.user.contains("hint:"));
    }

    #[test]
    fn the_rewording_prompt_recounts_every_earlier_wording_and_its_findings() {
        let mut first = candidate();
        first.title = "Numbers".into();
        let history = vec![(
            first,
            vec!["title: too vague to name the behavior".to_string()],
        )];
        let prompt = rewording_prompt(&candidate(), "criteria: only happy paths", &history);
        assert!(prompt.user.contains("Earlier wordings of this draft"));
        assert!(prompt.user.contains("Wording 1:\ntitle: Numbers"));
        assert!(
            prompt
                .user
                .contains("Wording 1 findings:\n- title: too vague to name the behavior")
        );
        assert!(prompt.user.contains("do not return to any of them"));
    }

    #[test]
    fn a_complete_rewording_object_parses_with_or_without_fences() {
        let reply =
            r#"{"title": "T", "story": "S", "acceptanceCriteria": ["Given x, when y, then z"]}"#;
        assert_eq!(parse_rewording(reply).unwrap().title, "T");
        let fenced = format!("```json\n{reply}\n```");
        assert!(parse_rewording(&fenced).is_some());
    }

    #[test]
    fn incomplete_or_unparseable_rewordings_are_none() {
        assert!(parse_rewording("Sure! Reworded below:").is_none());
        assert!(
            parse_rewording(r#"{"title": "", "story": "S", "acceptanceCriteria": ["x"]}"#)
                .is_none()
        );
        assert!(
            parse_rewording(r#"{"title": "T", "story": "S", "acceptanceCriteria": []}"#).is_none()
        );
        assert!(
            parse_rewording(r#"{"title": "T", "story": "S", "acceptanceCriteria": [" "]}"#)
                .is_none()
        );
    }

    #[test]
    fn the_prompt_carries_the_description_and_the_strict_rules() {
        let prompt = proposal_prompt("sum numbers from a string");
        assert!(prompt.user.contains("sum numbers from a string"));
        assert!(prompt.system.contains("ONLY a JSON array"));
        assert!(prompt.system.contains("acceptanceCriteria"));
        assert!(prompt.system.contains("Do not invent capabilities"));
        // A broad description is unpacked into its conventional parts,
        // never refused with an empty array.
        assert!(prompt.system.contains("unpack"));
        assert!(
            prompt
                .system
                .contains("rather than replying with an empty array")
        );
    }

    #[test]
    fn a_json_array_parses_into_proposals() {
        let reply = r#"[
            {"title": "Empty string returns zero",
             "story": "As a user, I want empty input to be 0 so that no input is safe.",
             "acceptanceCriteria": ["Given \"\", when add is called, then the result is 0"]},
            {"title": "Comma separated numbers are summed",
             "story": "As a user, I want comma sums so that totals come from one input.",
             "acceptanceCriteria": ["Given \"1,2\", when add is called, then the result is 3"]}
        ]"#;
        let proposals = parse_proposals(reply);
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].title, "Empty string returns zero");
        assert_eq!(proposals[1].acceptance_criteria.len(), 1);
    }

    #[test]
    fn a_code_fenced_reply_still_parses() {
        let reply = "```json\n[{\"title\": \"T\", \"story\": \"S\", \
                     \"acceptanceCriteria\": [\"Given x, when y, then z\"]}]\n```";
        assert_eq!(parse_proposals(reply).len(), 1);
    }

    #[test]
    fn prose_or_broken_json_is_an_empty_list() {
        assert!(parse_proposals("Sure! Here are the requirements:").is_empty());
        assert!(parse_proposals("[{\"title\": unclosed").is_empty());
    }

    #[test]
    fn incomplete_proposals_are_dropped() {
        let reply = r#"[
            {"title": "", "story": "S", "acceptanceCriteria": ["Given x, when y, then z"]},
            {"title": "T", "story": " ", "acceptanceCriteria": ["Given x, when y, then z"]},
            {"title": "T", "story": "S", "acceptanceCriteria": []},
            {"title": "T", "story": "S", "acceptanceCriteria": ["  "]},
            {"title": "Kept", "story": "S", "acceptanceCriteria": ["Given x, when y, then z"]}
        ]"#;
        let proposals = parse_proposals(reply);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].title, "Kept");
    }
}
