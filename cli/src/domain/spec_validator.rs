//! Structural validation of the requirements spec — the Rust port of the
//! Java server's `SpecValidator`. Issue strings match the Java output
//! verbatim, except where a finding names the CLI command that repairs
//! it (the missing-featureFile finding points at mark-implemented).

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

use crate::domain::model::{Requirement, Spec};
use crate::ports::FeatureFiles;

static ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9]*-\d+$").expect("valid regex"));

const STATUSES: [&str; 2] = ["pending", "implemented"];

/// Validates a spec document. Feature-file questions are answered through
/// the injected [`FeatureFiles`] port, keeping this logic free of IO.
pub struct SpecValidator<'a> {
    feature_files: &'a dyn FeatureFiles,
}

impl<'a> SpecValidator<'a> {
    pub fn new(feature_files: &'a dyn FeatureFiles) -> Self {
        Self { feature_files }
    }

    /// Returns an empty list when the spec is valid, otherwise every issue found.
    pub fn validate(&self, spec: &Spec) -> Vec<String> {
        let mut issues = Vec::new();
        if is_blank(&spec.project) {
            issues.push("spec: the project name is missing".to_string());
        }
        if spec.requirements.is_empty() {
            issues.push("spec: the requirements array is missing or empty".to_string());
            return issues;
        }
        let mut seen_ids = HashSet::new();
        for requirement in &spec.requirements {
            self.validate_requirement(requirement, &mut seen_ids, &mut issues);
        }
        issues
    }

    fn validate_requirement(
        &self,
        r: &Requirement,
        seen_ids: &mut HashSet<String>,
        issues: &mut Vec<String>,
    ) {
        let id = &r.id;
        if !ID.is_match(id) {
            issues.push(format!(
                "{id}: id must look like REQ-007 (uppercase prefix, dash, number)"
            ));
        }
        if !seen_ids.insert(id.clone()) {
            issues.push(format!(
                "{id}: duplicate id - every requirement needs its own"
            ));
        }
        if is_blank(&r.title) {
            issues.push(format!("{id}: title is missing"));
        }
        if is_blank(&r.story) {
            issues.push(format!("{id}: user story is missing"));
        }
        if r.acceptance_criteria.is_empty() {
            issues.push(format!(
                "{id}: at least one acceptance criterion is required"
            ));
        }
        for criterion in &r.acceptance_criteria {
            let lower = criterion.to_lowercase();
            if !lower.contains("given") || !lower.contains("when") || !lower.contains("then") {
                issues.push(format!(
                    "{id}: criterion \"{criterion}\" must be phrased Given/When/Then"
                ));
            }
        }
        if !STATUSES.contains(&r.status.to_lowercase().as_str()) {
            issues.push(format!("{id}: status must be 'pending' or 'implemented'"));
        }
        self.validate_feature_file(r, id, issues);
    }

    fn validate_feature_file(&self, r: &Requirement, id: &str, issues: &mut Vec<String>) {
        let feature = match r.feature_file.as_deref().filter(|f| !is_blank(f)) {
            None => {
                if !r.is_pending() {
                    issues.push(format!(
                        "{id}: implemented requirements must name their featureFile - \
                         rerun bdd spec mark-implemented {id} on GREEN to backfill it"
                    ));
                }
                return;
            }
            Some(feature) => feature,
        };
        if !self.feature_files.exists(feature) {
            issues.push(format!("{id}: featureFile {feature} does not exist"));
            return;
        }
        if !r.is_pending() && !self.feature_files.has_tag(feature, &format!("@{id}")) {
            issues.push(format!(
                "{id}: no scenario tagged @{id} in {feature} - implemented requirements \
                 need executable scenarios"
            ));
        }
    }
}

fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct FakeFeatures {
        existing: HashSet<String>,
        tags: HashMap<String, HashSet<String>>,
    }

    impl FeatureFiles for FakeFeatures {
        fn exists(&self, path: &str) -> bool {
            self.existing.contains(path)
        }
        fn has_tag(&self, path: &str, tag: &str) -> bool {
            self.tags.get(path).is_some_and(|t| t.contains(tag))
        }
    }

    fn requirement(id: &str) -> Requirement {
        Requirement {
            id: id.into(),
            title: "A title".into(),
            status: "pending".into(),
            story: "As a user, I want things so that value.".into(),
            acceptance_criteria: vec!["Given a, when b, then 3".into()],
            feature_file: Some("features/x.feature".into()),
        }
    }

    fn spec_with(requirements: Vec<Requirement>) -> Spec {
        Spec {
            project: "Kata".into(),
            description: None,
            requirements,
        }
    }

    fn features_with_file() -> FakeFeatures {
        let mut f = FakeFeatures::default();
        f.existing.insert("features/x.feature".into());
        f
    }

    #[test]
    fn a_well_formed_spec_is_valid() {
        let features = features_with_file();
        let issues =
            SpecValidator::new(&features).validate(&spec_with(vec![requirement("REQ-001")]));
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn an_empty_requirements_array_is_the_only_issue_reported() {
        let features = FakeFeatures::default();
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![]));
        assert_eq!(
            issues,
            vec!["spec: the requirements array is missing or empty"]
        );
    }

    #[test]
    fn a_missing_project_name_is_reported() {
        let features = features_with_file();
        let mut spec = spec_with(vec![requirement("REQ-001")]);
        spec.project = "  ".into();
        let issues = SpecValidator::new(&features).validate(&spec);
        assert_eq!(issues, vec!["spec: the project name is missing"]);
    }

    #[test]
    fn a_malformed_id_is_rejected() {
        let features = features_with_file();
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![requirement("req-1")]));
        assert_eq!(
            issues,
            vec!["req-1: id must look like REQ-007 (uppercase prefix, dash, number)"]
        );
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let features = features_with_file();
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![
            requirement("REQ-001"),
            requirement("REQ-001"),
        ]));
        assert_eq!(
            issues,
            vec!["REQ-001: duplicate id - every requirement needs its own"]
        );
    }

    #[test]
    fn missing_title_story_and_criteria_are_each_reported() {
        let features = features_with_file();
        let mut r = requirement("REQ-002");
        r.title = "".into();
        r.story = " ".into();
        r.acceptance_criteria.clear();
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![r]));
        assert_eq!(
            issues,
            vec![
                "REQ-002: title is missing",
                "REQ-002: user story is missing",
                "REQ-002: at least one acceptance criterion is required",
            ]
        );
    }

    #[test]
    fn a_criterion_without_given_when_then_is_rejected() {
        let features = features_with_file();
        let mut r = requirement("REQ-007");
        r.acceptance_criteria = vec!["the result should be 6 for 1\\n2,3".into()];
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![r]));
        assert_eq!(
            issues,
            vec![
                "REQ-007: criterion \"the result should be 6 for 1\\n2,3\" \
                 must be phrased Given/When/Then"
            ]
        );
    }

    #[test]
    fn given_when_then_detection_is_case_insensitive() {
        let features = features_with_file();
        let mut r = requirement("REQ-003");
        r.acceptance_criteria = vec!["GIVEN x, WHEN y, THEN 2".into()];
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![r]));
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn an_unknown_status_is_rejected() {
        let features = features_with_file();
        let mut r = requirement("REQ-004");
        r.status = "done".into();
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![r]));
        // An unknown status is also treated as not-pending, so the
        // tagged-scenario rule fires too — same as the Java validator.
        assert_eq!(
            issues,
            vec![
                "REQ-004: status must be 'pending' or 'implemented'",
                "REQ-004: no scenario tagged @REQ-004 in features/x.feature - \
                 implemented requirements need executable scenarios",
            ]
        );
    }

    #[test]
    fn a_pending_requirement_may_omit_its_feature_file() {
        let features = FakeFeatures::default();
        let mut r = requirement("REQ-005");
        r.feature_file = None;
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![r]));
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn an_implemented_requirement_must_name_its_feature_file() {
        let features = FakeFeatures::default();
        let mut r = requirement("REQ-005");
        r.status = "implemented".into();
        r.feature_file = None;
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![r]));
        assert_eq!(
            issues,
            vec![
                "REQ-005: implemented requirements must name their featureFile - \
                 rerun bdd spec mark-implemented REQ-005 on GREEN to backfill it"
            ]
        );
    }

    #[test]
    fn a_named_feature_file_must_exist() {
        let features = FakeFeatures::default();
        let issues =
            SpecValidator::new(&features).validate(&spec_with(vec![requirement("REQ-006")]));
        assert_eq!(
            issues,
            vec!["REQ-006: featureFile features/x.feature does not exist"]
        );
    }

    #[test]
    fn an_implemented_requirement_needs_a_tagged_scenario() {
        let features = features_with_file();
        let mut r = requirement("REQ-006");
        r.status = "implemented".into();
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![r]));
        assert_eq!(
            issues,
            vec![
                "REQ-006: no scenario tagged @REQ-006 in features/x.feature - \
                 implemented requirements need executable scenarios"
            ]
        );
    }

    #[test]
    fn an_implemented_requirement_with_a_tagged_scenario_is_valid() {
        let mut features = features_with_file();
        features
            .tags
            .entry("features/x.feature".into())
            .or_default()
            .insert("@REQ-006".into());
        let mut r = requirement("REQ-006");
        r.status = "implemented".into();
        let issues = SpecValidator::new(&features).validate(&spec_with(vec![r]));
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }
}
