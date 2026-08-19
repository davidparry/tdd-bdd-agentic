//! The requirement spec model — the same JSON shape the workshop's
//! `requirements/requirements.json` uses, so both servers read one spec.

use serde::{Deserialize, Serialize};

/// One requirement of the spec: the unit the whole workflow revolves
/// around (draft -> validate -> refine -> scenario -> tests -> code).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub story: String,
    #[serde(rename = "acceptanceCriteria", default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(
        rename = "featureFile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub feature_file: Option<String>,
}

impl Requirement {
    pub fn is_pending(&self) -> bool {
        self.status.eq_ignore_ascii_case("pending")
    }
}

/// The whole spec document: the source of truth the loop starts from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    #[serde(default)]
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
}

/// The outcome of a single test-suite execution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRunSummary {
    pub tests: u32,
    pub failures: u32,
    pub errors: u32,
    pub skipped: u32,
    #[serde(rename = "failureDetails")]
    pub failure_details: Vec<String>,
}

impl TestRunSummary {
    pub fn passed(&self) -> bool {
        self.tests > 0 && self.failures == 0 && self.errors == 0
    }

    /// A successful build that produced no reports — not GREEN (no tests
    /// ran) and not RED (nothing failed). Callers must not flip the
    /// phase to RED for this outcome.
    pub fn no_tests(&self) -> bool {
        self.tests == 0 && self.failures == 0 && self.errors == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_status_is_case_insensitive() {
        let mut r = requirement("REQ-001");
        r.status = "Pending".into();
        assert!(r.is_pending());
        r.status = "implemented".into();
        assert!(!r.is_pending());
    }

    #[test]
    fn a_run_with_no_tests_has_not_passed() {
        assert!(!TestRunSummary::default().passed());
        assert!(TestRunSummary::default().no_tests());
    }

    #[test]
    fn a_run_passes_only_when_tests_ran_and_nothing_failed() {
        let run = TestRunSummary {
            tests: 5,
            ..Default::default()
        };
        assert!(run.passed());
        let red = TestRunSummary {
            tests: 5,
            failures: 1,
            ..Default::default()
        };
        assert!(!red.passed());
        let error = TestRunSummary {
            tests: 5,
            errors: 1,
            ..Default::default()
        };
        assert!(!error.passed());
    }

    #[test]
    fn spec_round_trips_through_the_workshop_json_field_names() {
        let json = r#"{
            "project": "Kata",
            "requirements": [{
                "id": "REQ-001",
                "title": "T",
                "status": "pending",
                "story": "As a user, I want X so that Y.",
                "acceptanceCriteria": ["Given a, when b, then c"],
                "featureFile": "features/x.feature"
            }]
        }"#;
        let spec: Spec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.requirements[0].acceptance_criteria.len(), 1);
        assert_eq!(
            spec.requirements[0].feature_file.as_deref(),
            Some("features/x.feature")
        );
        let out = serde_json::to_string(&spec).unwrap();
        assert!(out.contains("acceptanceCriteria"));
        assert!(out.contains("featureFile"));
    }

    pub fn requirement(id: &str) -> Requirement {
        Requirement {
            id: id.into(),
            title: "A title".into(),
            status: "pending".into(),
            story: "As a user, I want things so that value.".into(),
            acceptance_criteria: vec!["Given a, when b, then 3".into()],
            feature_file: Some("features/x.feature".into()),
        }
    }
}
