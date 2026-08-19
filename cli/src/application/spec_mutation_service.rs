//! Spec mutations: interactive drafting (the human words the spec, the
//! validate → refine findings drive rewording until clean) and the
//! GREEN-gated `mark-implemented`. Every mutation lands in the staging
//! area, never directly in the working tree.

use serde::Serialize;

use crate::application::assets::feature_tagged;
use crate::application::spec_service::ServiceError;
use crate::domain::model::{Requirement, Spec};
use crate::domain::proposal::{
    ProposedRequirement, parse_proposals, parse_rewording, proposal_prompt, rewording_prompt,
};
use crate::domain::refiner::{RequirementRefiner, suggestion_for};
use crate::domain::spec_validator::SpecValidator;
use crate::domain::tdd::TddPhase;
use crate::ports::{
    ChangeStore, FeatureCatalog, FeatureFiles, LlmGenerator, Prompter, SpecRepository, StateStore,
};

/// A resolved model available to assist drafting: name + generator.
type ModelAid<'a> = Option<(&'a str, &'a dyn LlmGenerator)>;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DraftReport {
    pub id: String,
    pub title: String,
    pub staged: bool,
    #[serde(rename = "nextStep")]
    pub next_step: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SetFeatureReport {
    pub id: String,
    #[serde(rename = "featureFile")]
    pub feature_file: String,
    pub staged: bool,
    #[serde(rename = "nextStep")]
    pub next_step: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ListedRequirement {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub staged: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct MarkReport {
    pub id: String,
    pub status: String,
    pub staged: bool,
    #[serde(rename = "nextStep")]
    pub next_step: String,
}

pub struct SpecMutationService<
    R: SpecRepository,
    F: FeatureFiles,
    G: FeatureCatalog,
    C: ChangeStore,
    S: StateStore,
> {
    repository: R,
    feature_files: F,
    catalog: G,
    store: C,
    state: S,
    spec_path: String,
}

impl<R: SpecRepository, F: FeatureFiles, G: FeatureCatalog, C: ChangeStore, S: StateStore>
    SpecMutationService<R, F, G, C, S>
{
    pub fn new(
        repository: R,
        feature_files: F,
        catalog: G,
        store: C,
        state: S,
        spec_path: String,
    ) -> Self {
        Self {
            repository,
            feature_files,
            catalog,
            store,
            state,
            spec_path,
        }
    }

    /// Draft a new requirement interactively. The human words the spec;
    /// the loop reruns validate + refine on every wording until there
    /// are no findings, then asks for approval before staging. On
    /// rewording passes every prompt carries the prior answer, which
    /// Enter keeps as-is.
    pub fn draft(&self, prompter: &mut dyn Prompter) -> Result<DraftReport, ServiceError> {
        let spec = self.effective_spec()?;
        let id = next_id(&spec);
        self.manual_draft(prompter, spec, id, None)
    }

    /// Draft with the model's help: the human describes what to build
    /// in plain words, the model splits the description into complete
    /// requirement proposals (title, story, criteria), the human picks
    /// one, and the wizard walks through every field with the proposal
    /// pre-filled - Enter accepts, typing replaces. Any model failure
    /// falls back to manual drafting.
    pub fn draft_assisted(
        &self,
        prompter: &mut dyn Prompter,
        model: &str,
        llm: &dyn LlmGenerator,
    ) -> Result<DraftReport, ServiceError> {
        let spec = self.effective_spec()?;
        let id = next_id(&spec);
        let description = self.ask(
            prompter,
            "Describe what to build in plain words (one or several requirements). \
             Enter drafts manually instead:",
        )?;
        if description.is_empty() {
            return self.manual_draft(prompter, spec, id, Some((model, llm)));
        }
        let work = prompter.working(&format!(
            "Splitting the description into requirements with {model} - working"
        ));
        let prompt = proposal_prompt(&description);
        tracing::debug!(purpose = "spec draft proposal", "calling LLM");
        let reply = llm.generate(model, &prompt.system, &prompt.user);
        drop(work);
        let mut proposals = match reply {
            Ok(reply) => parse_proposals(&reply),
            Err(error) => {
                prompter.warn(&format!("The model call failed ({}).", error.0));
                Vec::new()
            }
        };
        if proposals.is_empty() {
            prompter.warn("The description gave no complete requirement - drafting manually.");
            return self.manual_draft(prompter, spec, id, Some((model, llm)));
        }
        prompter.tell(&format!(
            "The description holds {} requirement(s):",
            proposals.len()
        ));
        for (index, proposal) in proposals.iter().enumerate() {
            prompter.tell(&format!("  {}. {}", index + 1, proposal.title));
        }
        let chosen = self.pick_proposal(prompter, proposals.len())?;
        let ProposedRequirement {
            title,
            story,
            acceptance_criteria,
        } = proposals.remove(chosen);
        let rest: Vec<String> = proposals.into_iter().map(|p| p.title).collect();
        if !rest.is_empty() {
            prompter.tell(&format!(
                "Left for later runs: {}. Draft them the same way afterwards.",
                rest.join(", ")
            ));
        }
        prompter.tell(&format!(
            "Walking through {id}. Each prompt shows the proposal - Enter accepts \
             it, or type your own wording."
        ));
        let proposal = Requirement {
            id: id.clone(),
            title,
            status: "pending".into(),
            story,
            acceptance_criteria,
            feature_file: None,
        };
        self.draft_loop(
            prompter,
            spec,
            id,
            Some(proposal),
            Some((model, llm)),
            false,
        )
    }

    /// Which proposal to start with. Enter means the first; anything
    /// else must be a number from the list.
    fn pick_proposal(
        &self,
        prompter: &mut dyn Prompter,
        count: usize,
    ) -> Result<usize, ServiceError> {
        if count == 1 {
            return Ok(0);
        }
        loop {
            let answer = self.ask(
                prompter,
                &format!("Which requirement first? [1-{count}, Enter for 1]:"),
            )?;
            if answer.is_empty() {
                return Ok(0);
            }
            match answer.parse::<usize>() {
                Ok(pick) if (1..=count).contains(&pick) => return Ok(pick - 1),
                _ => prompter.warn(&format!("Pick a number between 1 and {count}.")),
            }
        }
    }

    fn manual_draft(
        &self,
        prompter: &mut dyn Prompter,
        spec: Spec,
        id: String,
        llm: ModelAid,
    ) -> Result<DraftReport, ServiceError> {
        prompter.tell(&format!(
            "Drafting {id}. You word the spec; validate and refine findings drive \
             rewording until the wording is clean."
        ));
        self.draft_loop(prompter, spec, id, None, llm, false)
    }

    /// Reword an existing requirement interactively (refine findings on
    /// a backlog item). Stages a replacement of that row.
    pub fn reword(
        &self,
        prompter: &mut dyn Prompter,
        id: &str,
    ) -> Result<DraftReport, ServiceError> {
        self.reword_with(prompter, id, None)
    }

    pub fn reword_assisted(
        &self,
        prompter: &mut dyn Prompter,
        id: &str,
        model: &str,
        llm: &dyn LlmGenerator,
    ) -> Result<DraftReport, ServiceError> {
        self.reword_with(prompter, id, Some((model, llm)))
    }

    fn reword_with(
        &self,
        prompter: &mut dyn Prompter,
        id: &str,
        llm: ModelAid,
    ) -> Result<DraftReport, ServiceError> {
        let spec = self.effective_spec()?;
        let existing = spec
            .requirements
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| {
                ServiceError(format!(
                    "No requirement with id '{id}'. Call spec list to see valid ids."
                ))
            })?;
        prompter.tell(&format!(
            "Rewording {id}. You word the spec; validate and refine findings drive \
             rewording until the wording is clean."
        ));
        self.draft_loop(prompter, spec, id.to_string(), Some(existing), llm, true)
    }

    /// Non-interactive draft from flags. Structural validate must pass;
    /// refine findings are reported in `nextStep` rather than blocking.
    pub fn draft_direct(
        &self,
        title: &str,
        story: &str,
        criteria: Vec<String>,
    ) -> Result<DraftReport, ServiceError> {
        let mut spec = self.effective_spec()?;
        let id = next_id(&spec);
        let candidate = Requirement {
            id: id.clone(),
            title: title.to_string(),
            status: "pending".into(),
            story: story.to_string(),
            acceptance_criteria: criteria,
            feature_file: None,
        };
        self.stage_direct(&mut spec, candidate, false)
    }

    /// Non-interactive reword of an existing requirement from flags.
    pub fn reword_direct(
        &self,
        id: &str,
        title: Option<String>,
        story: Option<String>,
        criteria: Vec<String>,
    ) -> Result<DraftReport, ServiceError> {
        let mut spec = self.effective_spec()?;
        let mut candidate = spec
            .requirements
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| {
                ServiceError(format!(
                    "No requirement with id '{id}'. Call spec list to see valid ids."
                ))
            })?;
        if let Some(title) = title {
            candidate.title = title;
        }
        if let Some(story) = story {
            candidate.story = story;
        }
        if !criteria.is_empty() {
            candidate.acceptance_criteria = criteria;
        }
        self.stage_direct(&mut spec, candidate, true)
    }

    fn stage_direct(
        &self,
        spec: &mut Spec,
        candidate: Requirement,
        replace: bool,
    ) -> Result<DraftReport, ServiceError> {
        let id = candidate.id.clone();
        let title = candidate.title.clone();
        let findings = self.findings_for(spec, &candidate);
        let structural: Vec<String> = findings
            .iter()
            .filter(|issue| {
                issue.contains("must be phrased")
                    || issue.contains("title is missing")
                    || issue.contains("user story is missing")
                    || issue.contains("at least one acceptance criterion")
                    || issue.contains("id must look like")
                    || issue.contains("duplicate id")
            })
            .cloned()
            .collect();
        if !structural.is_empty() {
            return Err(ServiceError(structural.join(" ")));
        }
        let warning = duplicate_warning(spec, &candidate);
        if replace {
            if let Some(slot) = spec.requirements.iter_mut().find(|r| r.id == id) {
                *slot = candidate;
            }
            self.stage_spec(spec, &format!("reword {id}"))?;
        } else {
            spec.requirements.push(candidate);
            self.stage_spec(spec, &format!("draft {id}: {title}"))?;
        }
        let mut next_step = format!(
            "Review with bdd changes show and apply with bdd changes commit, then add \
             the @{id} scenario with bdd scenario add."
        );
        let refine: Vec<String> = findings
            .into_iter()
            .filter(|issue| {
                !issue.contains("must be phrased") && !issue.contains("title is missing")
            })
            .collect();
        if !refine.is_empty() {
            next_step = format!(
                "Staged {id} with refine findings. Run bdd spec reword {id} to address \
                 them, then bdd changes commit."
            );
        }
        if let Some(warning) = warning {
            next_step = format!("{warning} {next_step}");
        }
        Ok(DraftReport {
            id,
            title,
            staged: true,
            next_step,
        })
    }

    /// Point a requirement at the feature file that was actually written.
    /// No-op (and unstaged) when the path is already recorded.
    pub fn set_feature(&self, id: &str, path: &str) -> Result<SetFeatureReport, ServiceError> {
        let mut spec = self.effective_spec()?;
        let requirement = spec
            .requirements
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| {
                ServiceError(format!(
                    "No requirement with id '{id}'. Call spec list to see valid ids."
                ))
            })?;
        if requirement.feature_file.as_deref() == Some(path) {
            return Ok(SetFeatureReport {
                id: id.to_string(),
                feature_file: path.to_string(),
                staged: false,
                next_step: format!("{id} already names {path}."),
            });
        }
        requirement.feature_file = Some(path.to_string());
        self.stage_spec(&spec, &format!("set {id} featureFile to {path}"))?;
        Ok(SetFeatureReport {
            id: id.to_string(),
            feature_file: path.to_string(),
            staged: true,
            next_step: "Review with bdd changes show, then apply with bdd changes commit."
                .to_string(),
        })
    }

    /// Every requirement on the effective (staged-wins) spec. New ids
    /// that exist only in staging are labelled `staged`.
    pub fn list_requirements(&self) -> Result<Vec<ListedRequirement>, ServiceError> {
        let disk_ids: std::collections::HashSet<String> = self
            .repository
            .load()
            .map(|spec| spec.requirements.into_iter().map(|r| r.id).collect())
            .unwrap_or_default();
        let spec = self.effective_spec()?;
        Ok(spec
            .requirements
            .into_iter()
            .map(|r| ListedRequirement {
                staged: !disk_ids.contains(&r.id),
                id: r.id,
                title: r.title,
                status: r.status,
            })
            .collect())
    }

    /// The wizard loop shared by manual and assisted drafting. `prior`
    /// pre-fills every prompt (a model proposal or the previous pass's
    /// answers); validate + refine findings drive rewording until clean.
    fn draft_loop(
        &self,
        prompter: &mut dyn Prompter,
        mut spec: Spec,
        id: String,
        mut prior: Option<Requirement>,
        llm: ModelAid,
        replace: bool,
    ) -> Result<DraftReport, ServiceError> {
        // Every rejected wording and its findings, oldest first: the
        // model's rewording brief carries them so it never circles back
        // to a wording the review already rejected.
        let mut tries: Vec<(Requirement, Vec<String>)> = Vec::new();
        let (requirement, title) = loop {
            let title = self.ask_field(
                prompter,
                &id,
                "title",
                prior.as_ref().map(|p| p.title.as_str()),
            )?;
            let story = self.ask_field(
                prompter,
                &id,
                "story (As a ..., I want ..., so that ...)",
                prior.as_ref().map(|p| p.story.as_str()),
            )?;
            let prior_criteria = prior
                .as_ref()
                .map(|p| p.acceptance_criteria.as_slice())
                .unwrap_or(&[]);
            let criteria = self.ask_criteria(prompter, &id, prior_criteria)?;
            let candidate = Requirement {
                id: id.clone(),
                title: title.clone(),
                status: "pending".into(),
                story,
                acceptance_criteria: criteria,
                feature_file: prior.as_ref().and_then(|p| p.feature_file.clone()),
            };
            let findings = self.findings_for(&spec, &candidate);
            if findings.is_empty() {
                break (candidate, title);
            }
            prompter.tell("Findings to address:");
            for finding in &findings {
                prompter.tell(&format!("  - {finding}"));
                if let Some(suggestion) = suggestion_for(finding) {
                    prompter.tell(&format!("    try: {suggestion}"));
                }
            }
            // With a model, the findings become its brief: the next pass's
            // prompts carry its reworded proposal instead of the raw prior.
            let reworded = llm.and_then(|(model, llm)| {
                self.rewording(prompter, model, llm, &candidate, &findings, &tries)
            });
            tries.push((candidate.clone(), findings));
            prior = Some(match reworded {
                Some(requirement) => requirement,
                None => {
                    prompter.tell(
                        "Reword the requirement to address each finding. Press Enter \
                         on a prompt to keep the prior answer as-is.",
                    );
                    candidate
                }
            });
        };
        if !self.confirm(prompter, "The wording reads clean. Stage this requirement?")? {
            return Ok(DraftReport {
                id,
                title,
                staged: false,
                next_step: "Nothing was staged. Run spec draft again when the wording \
                            is ready."
                    .into(),
            });
        }
        if replace {
            if let Some(slot) = spec.requirements.iter_mut().find(|r| r.id == id) {
                *slot = requirement;
            }
            self.stage_spec(&spec, &format!("reword {id}"))?;
        } else {
            spec.requirements.push(requirement);
            self.stage_spec(&spec, &format!("draft {id}: {title}"))?;
        }
        Ok(DraftReport {
            id: id.clone(),
            title,
            staged: true,
            next_step: format!(
                "Review with changes show and apply with changes commit, then add \
                 the @{id} scenario with scenario add."
            ),
        })
    }

    /// Ask the model to reword the draft, one call per finding: each
    /// call addresses exactly one finding and is briefed with the draft
    /// the previous call produced, so the fixes accumulate. An unusable
    /// reply skips that finding; a model error ends the chain (a dead
    /// model would fail every remaining call too). `None` when no call
    /// landed a fix - the developer rewords by hand, exactly as without
    /// a model.
    fn rewording(
        &self,
        prompter: &mut dyn Prompter,
        model: &str,
        llm: &dyn LlmGenerator,
        candidate: &Requirement,
        findings: &[String],
        history: &[(Requirement, Vec<String>)],
    ) -> Option<Requirement> {
        let mut current = candidate.clone();
        let mut applied = false;
        let total = findings.len();
        for (index, finding) in findings.iter().enumerate() {
            let work = prompter.working(&format!(
                "Asking {model} to address finding {n} of {total} - working",
                n = index + 1
            ));
            let prompt = rewording_prompt(&current, finding, history);
            tracing::debug!(purpose = "requirement rewording", "calling LLM");
            let outcome = llm.generate(model, &prompt.system, &prompt.user);
            drop(work);
            let reply = match outcome {
                Ok(reply) => reply,
                Err(error) => {
                    prompter.warn(&format!("The model call failed ({}).", error.0));
                    break;
                }
            };
            match parse_rewording(&reply) {
                Some(proposal) => {
                    current = Requirement {
                        id: candidate.id.clone(),
                        title: proposal.title,
                        status: candidate.status.clone(),
                        story: proposal.story,
                        acceptance_criteria: proposal.acceptance_criteria,
                        feature_file: candidate.feature_file.clone(),
                    };
                    applied = true;
                }
                None => prompter.warn(&format!(
                    "The model's rewording for finding {} was unusable - it stays \
                     yours to fix.",
                    index + 1
                )),
            }
        }
        if !applied {
            return None;
        }
        prompter.tell(
            "The model reworded the draft. Each prompt shows its proposal - Enter \
             accepts it, or type your own wording.",
        );
        Some(current)
    }

    /// Flip a requirement to implemented and record its featureFile from
    /// the `@REQ-ID`-tagged feature - the validator demands both, so the
    /// staged spec validates. Refused off GREEN (the status change is the
    /// last step of a passing loop, never a promise) and refused without
    /// a tagged scenario (an implemented requirement needs its executable
    /// scenario). Re-running on an already-implemented requirement
    /// backfills a missing featureFile.
    pub fn mark_implemented(&self, id: &str) -> Result<MarkReport, ServiceError> {
        let snapshot = self.state.load().map_err(|e| ServiceError(e.0))?;
        if snapshot.phase() != TddPhase::Green {
            return Err(ServiceError(format!(
                "Requirements are only marked implemented on GREEN (current phase: \
                 {}). Run the tests and make them pass first.",
                snapshot.phase()
            )));
        }
        let mut spec = self.effective_spec()?;
        let requirement = spec
            .requirements
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| {
                ServiceError(format!(
                    "No requirement with id '{id}'. Call list_requirements to see valid ids."
                ))
            })?;
        let feature = feature_tagged(&self.catalog, &format!("@{id}"))?.ok_or_else(|| {
            ServiceError(format!(
                "No scenario is tagged @{id} - implemented requirements need an \
                 executable scenario. Add one with bdd scenario add, apply it with \
                 bdd changes commit, then mark {id} implemented."
            ))
        })?;
        requirement.status = "implemented".into();
        requirement.feature_file = Some(feature);
        self.stage_spec(&spec, &format!("mark {id} implemented"))?;
        Ok(MarkReport {
            id: id.to_string(),
            status: "implemented".into(),
            staged: true,
            next_step: format!(
                "Review with changes show, run bdd validate (it checks the @{id} \
                 scenario exists), then bdd changes commit."
            ),
        })
    }

    /// The spec as it would look after commit: staged content wins over
    /// the working tree, so consecutive drafts stack.
    fn effective_spec(&self) -> Result<Spec, ServiceError> {
        match self
            .store
            .content(&self.spec_path)
            .map_err(|e| ServiceError(e.0))?
        {
            Some(text) => serde_json::from_str(&text).map_err(|e| {
                ServiceError(format!(
                    "spec: staged {} is not readable JSON - {e}",
                    self.spec_path
                ))
            }),
            None => self.repository.load().map_err(|e| ServiceError(e.0)),
        }
    }

    fn findings_for(&self, spec: &Spec, candidate: &Requirement) -> Vec<String> {
        let mut with_candidate = spec.clone();
        if let Some(existing) = with_candidate
            .requirements
            .iter_mut()
            .find(|r| r.id == candidate.id)
        {
            *existing = candidate.clone();
        } else {
            with_candidate.requirements.push(candidate.clone());
        }
        let prefix = format!("{}:", candidate.id);
        let mut findings: Vec<String> = SpecValidator::new(&self.feature_files)
            .validate(&with_candidate)
            .into_iter()
            .filter(|issue| issue.starts_with(&prefix))
            .collect();
        findings.extend(RequirementRefiner.review(candidate));
        findings
    }

    fn stage_spec(&self, spec: &Spec, summary: &str) -> Result<(), ServiceError> {
        let json = serde_json::to_string_pretty(spec).expect("spec is always serializable");
        self.store
            .stage(&self.spec_path, &json, summary)
            .map_err(|e| ServiceError(e.0))?;
        Ok(())
    }

    fn ask(&self, prompter: &mut dyn Prompter, question: &str) -> Result<String, ServiceError> {
        prompter.ask(question).map_err(|e| ServiceError(e.0))
    }

    /// One named field of the requirement. Rewording passes show the prior
    /// answer, and Enter keeps it unchanged.
    fn ask_field(
        &self,
        prompter: &mut dyn Prompter,
        id: &str,
        label: &str,
        prior: Option<&str>,
    ) -> Result<String, ServiceError> {
        match prior {
            None => self.ask(prompter, &format!("{id} {label}:")),
            Some(prior) => {
                let answer = self.ask(
                    prompter,
                    &format!("{id} {label} [{prior}] (Enter keeps it):"),
                )?;
                Ok(if answer.is_empty() {
                    prior.to_string()
                } else {
                    answer
                })
            }
        }
    }

    /// The acceptance criteria list. Prior criteria are offered one by one
    /// (Enter keeps, '-' drops); after them, each blank answer ends the
    /// list — the prompts say so.
    fn ask_criteria(
        &self,
        prompter: &mut dyn Prompter,
        id: &str,
        prior: &[String],
    ) -> Result<Vec<String>, ServiceError> {
        prompter.tell("Acceptance criteria (Given/When/Then). A blank criterion ends the list:");
        let mut criteria = Vec::new();
        for prior_criterion in prior {
            let answer = self.ask(
                prompter,
                &format!(
                    "{id} criterion {} [{prior_criterion}] (Enter keeps it, '-' drops it):",
                    criteria.len() + 1
                ),
            )?;
            match answer.as_str() {
                "" => criteria.push(prior_criterion.clone()),
                "-" => {}
                _ => criteria.push(answer),
            }
        }
        loop {
            let question = format!(
                "{id} criterion {} (leave blank to finish the criteria):",
                criteria.len() + 1
            );
            let criterion = self.ask(prompter, &question)?;
            if criterion.is_empty() {
                break;
            }
            criteria.push(criterion);
        }
        Ok(criteria)
    }

    fn confirm(&self, prompter: &mut dyn Prompter, question: &str) -> Result<bool, ServiceError> {
        prompter.confirm(question).map_err(|e| ServiceError(e.0))
    }
}

/// The next free REQ-### id, counting past staged drafts too.
fn next_id(spec: &Spec) -> String {
    let max = spec
        .requirements
        .iter()
        .filter_map(|r| {
            r.id.strip_prefix("REQ-")
                .and_then(|n| n.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0);
    format!("REQ-{:03}", max + 1)
}

fn duplicate_warning(spec: &Spec, candidate: &Requirement) -> Option<String> {
    spec.requirements
        .iter()
        .find(|existing| {
            existing.id != candidate.id
                && (existing.title.eq_ignore_ascii_case(&candidate.title)
                    || existing
                        .acceptance_criteria
                        .iter()
                        .any(|e| candidate.acceptance_criteria.iter().any(|c| c == e)))
        })
        .map(|existing| {
            format!(
                "Warning: {id} looks similar to {} ({}). ",
                existing.id,
                existing.title,
                id = candidate.id
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tdd::TddSnapshot;
    use crate::ports::{PromptError, SpecError};
    use crate::test_support::{
        FakeFeatureFiles, FixedStateStore, InMemoryChangeStore, InMemoryFeatureCatalog,
        InMemorySpecRepository, calculator_catalog,
    };
    use std::collections::VecDeque;

    #[derive(Default)]
    struct ScriptedPrompter {
        answers: VecDeque<String>,
        transcript: Vec<String>,
    }

    impl ScriptedPrompter {
        fn answering(answers: &[&str]) -> Self {
            Self {
                answers: answers.iter().map(|a| a.to_string()).collect(),
                transcript: Vec::new(),
            }
        }
    }

    impl Prompter for ScriptedPrompter {
        fn tell(&mut self, message: &str) {
            self.transcript.push(message.to_string());
        }

        fn ask(&mut self, question: &str) -> Result<String, PromptError> {
            self.transcript.push(question.to_string());
            self.answers
                .pop_front()
                .ok_or_else(|| PromptError("input is not readable - script exhausted".into()))
        }

        fn confirm(&mut self, question: &str) -> Result<bool, PromptError> {
            Ok(self.ask(question)?.eq_ignore_ascii_case("y"))
        }
    }

    const SPEC_PATH: &str = "requirements/requirements.json";

    fn requirement(id: &str) -> Requirement {
        Requirement {
            id: id.into(),
            title: "A title".into(),
            status: "pending".into(),
            story: "As a user, I want things so that value.".into(),
            acceptance_criteria: vec![
                "Given an empty string \"\", when add is called, then the result is 0".into(),
            ],
            feature_file: None,
        }
    }

    fn spec() -> Spec {
        Spec {
            project: "Kata".into(),
            description: None,
            requirements: vec![requirement("REQ-001"), requirement("REQ-007")],
        }
    }

    fn green() -> FixedStateStore {
        FixedStateStore::holding(TddSnapshot::at(TddPhase::Green))
    }

    fn service(
        spec: Result<Spec, SpecError>,
        state: FixedStateStore,
    ) -> SpecMutationService<
        InMemorySpecRepository,
        FakeFeatureFiles,
        InMemoryFeatureCatalog,
        InMemoryChangeStore,
        FixedStateStore,
    > {
        // The catalog carries the @REQ-001 tag in features/calc.feature;
        // REQ-007 has no tagged scenario anywhere.
        SpecMutationService::new(
            InMemorySpecRepository(spec),
            FakeFeatureFiles::default(),
            calculator_catalog(),
            InMemoryChangeStore::default(),
            state,
            SPEC_PATH.into(),
        )
    }

    const CLEAN_STORY: &str = "As a user, I want comma sums so that totals come from one input.";
    const CLEAN_CRITERION: &str =
        "Given the input \"1,2\", when add is called, then the result is 3";
    // The refiner's coverage rule wants at least one edge case.
    const EDGE_CRITERION: &str =
        "Given an empty string \"\", when add is called, then the result is 0";

    #[test]
    fn a_clean_draft_is_staged_with_the_next_free_id() {
        let service = service(Ok(spec()), green());
        let mut prompter = ScriptedPrompter::answering(&[
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service.draft(&mut prompter).unwrap();
        assert_eq!(report.id, "REQ-008");
        assert!(report.staged);
        assert!(report.next_step.contains("scenario add"));
        let staged = service.store.content(SPEC_PATH).unwrap().unwrap();
        let staged_spec: Spec = serde_json::from_str(&staged).unwrap();
        assert_eq!(staged_spec.requirements.len(), 3);
        assert_eq!(staged_spec.requirements[2].id, "REQ-008");
        assert_eq!(staged_spec.requirements[2].status, "pending");
        assert_eq!(service.store.summaries()[0], "draft REQ-008: Comma sums");
    }

    #[test]
    fn findings_are_told_and_the_human_rewords_until_clean() {
        let service = service(Ok(spec()), green());
        let mut prompter = ScriptedPrompter::answering(&[
            // first pass: vague story, criterion without Given/When/Then
            "Comma sums",
            "the calculator should handle commas quickly",
            "the result is 3",
            "",
            // second pass: clean
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service.draft(&mut prompter).unwrap();
        assert!(report.staged);
        assert!(
            prompter
                .transcript
                .iter()
                .any(|l| l == "Findings to address:"),
            "transcript: {:#?}",
            prompter.transcript
        );
        assert!(
            prompter
                .transcript
                .iter()
                .any(|l| l.contains("must be phrased Given/When/Then")),
            "transcript: {:#?}",
            prompter.transcript
        );
        // Every finding with an obvious fix carries a "try:" suggestion.
        assert!(
            prompter
                .transcript
                .iter()
                .any(|l| l.contains("try: rephrase as: Given <starting state>")),
            "transcript: {:#?}",
            prompter.transcript
        );
    }

    #[test]
    fn rewording_prompts_carry_the_id_and_the_prior_answers() {
        let service = service(Ok(spec()), green());
        let mut prompter = ScriptedPrompter::answering(&[
            "Comma sums",
            "the calculator should handle commas quickly",
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "",
            CLEAN_STORY,
            "",
            "",
            "",
            "y",
        ]);
        let report = service.draft(&mut prompter).unwrap();
        assert!(report.staged, "report: {report:?}");
        let asked = |question: &str| {
            assert!(
                prompter.transcript.iter().any(|l| l == question),
                "missing {question:?} in transcript: {:#?}",
                prompter.transcript
            );
        };
        asked("REQ-008 title:");
        asked("REQ-008 title [Comma sums] (Enter keeps it):");
        asked(&format!(
            "REQ-008 criterion 1 [{CLEAN_CRITERION}] (Enter keeps it, '-' drops it):"
        ));
        asked("REQ-008 criterion 3 (leave blank to finish the criteria):");
        // Enter kept the prior title, story was replaced, both criteria kept.
        let staged: Spec =
            serde_json::from_str(&service.store.content(SPEC_PATH).unwrap().unwrap()).unwrap();
        let drafted = &staged.requirements[2];
        assert_eq!(drafted.title, "Comma sums");
        assert_eq!(drafted.story, CLEAN_STORY);
        assert_eq!(
            drafted.acceptance_criteria,
            vec![CLEAN_CRITERION.to_string(), EDGE_CRITERION.to_string()]
        );
    }

    #[test]
    fn a_dash_drops_a_prior_criterion_on_the_rewording_pass() {
        let service = service(Ok(spec()), green());
        let mut prompter = ScriptedPrompter::answering(&[
            // first pass: one malformed criterion plus a clean edge case
            "Comma sums",
            CLEAN_STORY,
            "the result is 3",
            EDGE_CRITERION,
            "",
            // second pass: keep title and story, drop the malformed
            // criterion, keep the edge case, add a clean happy path
            "",
            "",
            "-",
            "",
            CLEAN_CRITERION,
            "",
            "y",
        ]);
        let report = service.draft(&mut prompter).unwrap();
        assert!(report.staged, "report: {report:?}");
        let staged: Spec =
            serde_json::from_str(&service.store.content(SPEC_PATH).unwrap().unwrap()).unwrap();
        assert_eq!(
            staged.requirements[2].acceptance_criteria,
            vec![EDGE_CRITERION.to_string(), CLEAN_CRITERION.to_string()]
        );
    }

    /// [`LlmGenerator`] with one scripted outcome.
    struct FakeLlm(Result<String, String>);

    impl LlmGenerator for FakeLlm {
        fn generate(
            &self,
            _model: &str,
            _system: &str,
            _user: &str,
        ) -> Result<String, crate::ports::LlmError> {
            self.0.clone().map_err(crate::ports::LlmError)
        }
    }

    const PROPOSALS: &str = r#"[
        {"title": "Comma separated numbers are summed",
         "story": "As a user, I want comma sums so that totals come from one input.",
         "acceptanceCriteria": ["Given the input \"1,2\", when add is called, then the result is 3"]},
        {"title": "Empty string returns zero",
         "story": "As a user, I want empty input to be 0 so that no input is a safe default.",
         "acceptanceCriteria": ["Given an empty string \"\", when add is called, then the result is 0"]}
    ]"#;

    #[test]
    fn a_blank_description_drafts_manually_without_calling_the_model() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Err("must not be called".into()));
        let mut prompter = ScriptedPrompter::answering(&[
            "",
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert!(
            !prompter.transcript.iter().any(|l| l.contains("working")),
            "transcript: {:#?}",
            prompter.transcript
        );
    }

    const REWORDING: &str = r#"{
        "title": "Comma separated numbers are summed",
        "story": "As a user, I want comma sums so that totals come from one input.",
        "acceptanceCriteria": [
            "Given the input \"1,2\", when add is called, then the result is 3",
            "Given an empty string \"\", when add is called, then the result is 0"
        ]
    }"#;

    #[test]
    fn findings_send_the_draft_to_the_model_whose_rewording_seeds_the_next_pass() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok(REWORDING.into()));
        // Blank description -> manual first pass with only a happy path,
        // then every rewording prompt accepts the model's proposal.
        let mut prompter = ScriptedPrompter::answering(&[
            "",
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            "",
            "",
            "",
            "",
            "",
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert_eq!(report.title, "Comma separated numbers are summed");
        let told = |line: &str| {
            assert!(
                prompter.transcript.iter().any(|l| l == line),
                "missing {line:?} in transcript: {:#?}",
                prompter.transcript
            );
        };
        told("Asking test-model to address finding 1 of 1 - working ...");
        told(
            "The model reworded the draft. Each prompt shows its proposal - Enter \
             accepts it, or type your own wording.",
        );
        // The rewording prompt carries the model's title, not the raw prior.
        told("REQ-008 title [Comma separated numbers are summed] (Enter keeps it):");
        let staged: Spec =
            serde_json::from_str(&service.store.content(SPEC_PATH).unwrap().unwrap()).unwrap();
        let drafted = staged.requirements.last().unwrap();
        assert_eq!(drafted.acceptance_criteria.len(), 2);
        assert_eq!(drafted.acceptance_criteria[1], EDGE_CRITERION);
    }

    /// [`LlmGenerator`] recording every call's system and user prompt
    /// (joined with a newline), replying the same thing.
    struct RecordingLlm {
        reply: String,
        prompts: std::cell::RefCell<Vec<String>>,
    }

    impl LlmGenerator for RecordingLlm {
        fn generate(
            &self,
            _model: &str,
            system: &str,
            user: &str,
        ) -> Result<String, crate::ports::LlmError> {
            self.prompts.borrow_mut().push(format!("{system}\n{user}"));
            Ok(self.reply.clone())
        }
    }

    /// A rewording that still only covers the happy path, so the review
    /// rejects it again and a second rewording pass runs.
    const FLAWED_REWORDING: &str = r#"{
        "title": "Comma separated numbers are summed",
        "story": "As a user, I want comma sums so that totals come from one input.",
        "acceptanceCriteria": ["Given the input \"1,2\", when add is called, then the result is 3"]
    }"#;

    #[test]
    fn a_second_rewording_pass_recounts_the_wording_the_review_already_rejected() {
        let service = service(Ok(spec()), green());
        let llm = RecordingLlm {
            reply: FLAWED_REWORDING.into(),
            prompts: std::cell::RefCell::new(Vec::new()),
        };
        let mut prompter = ScriptedPrompter::answering(&[
            "",              // blank description -> manual first pass
            "Comma sums",    // pass 1: title
            CLEAN_STORY,     // pass 1: story
            CLEAN_CRITERION, // pass 1: only the happy path -> findings
            "",
            "",
            "",
            "",
            "", // pass 2 accepts the model's (still flawed) rewording
            "",
            "",
            "",             // pass 3: keep title, story, criterion 1
            EDGE_CRITERION, // pass 3: the human adds the edge case
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        let prompts = llm.prompts.borrow();
        assert_eq!(prompts.len(), 2, "two rewording passes ran");
        assert!(
            !prompts[0].contains("Earlier wordings"),
            "the first pass has no history"
        );
        assert!(prompts[1].contains("Earlier wordings of this draft"));
        assert!(
            prompts[1].contains("title: Comma sums"),
            "prompt: {}",
            prompts[1]
        );
        assert!(prompts[1].contains("Wording 1 findings:"));
    }

    #[test]
    fn each_finding_is_its_own_model_call_briefed_with_the_previous_fix() {
        let service = service(Ok(spec()), green());
        let llm = RecordingLlm {
            reply: REWORDING.into(),
            prompts: std::cell::RefCell::new(Vec::new()),
        };
        // Pass 1: a story without an actor plus only a happy path -> two
        // findings, so the rewording chain makes two calls.
        let mut prompter = ScriptedPrompter::answering(&[
            "",           // blank description -> manual first pass
            "Comma sums", // pass 1: title
            "The user gets comma sums so that totals come from one input",
            CLEAN_CRITERION,
            "",
            "", // pass 2 accepts the reworded proposal field by field
            "",
            "",
            "",
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        let prompts = llm.prompts.borrow();
        assert_eq!(prompts.len(), 2, "one call per finding");
        assert!(
            prompts[0].contains("story: The user gets comma sums"),
            "the first call is briefed with the raw draft: {}",
            prompts[0]
        );
        assert!(
            prompts[0].contains("- story: missing the actor"),
            "prompt: {}",
            prompts[0]
        );
        assert!(
            prompts[1].contains("story: As a user, I want comma sums"),
            "the second call is briefed with the first call's fix: {}",
            prompts[1]
        );
        assert!(
            prompts[1].contains("- criteria: only happy paths"),
            "prompt: {}",
            prompts[1]
        );
        let told = |line: &str| {
            assert!(
                prompter.transcript.iter().any(|l| l == line),
                "missing {line:?} in transcript: {:#?}",
                prompter.transcript
            );
        };
        told("Asking test-model to address finding 1 of 2 - working ...");
        told("Asking test-model to address finding 2 of 2 - working ...");
    }

    /// [`LlmGenerator`] that answers the first call and fails the rest.
    struct FlakyLlm {
        reply: String,
        calls: std::cell::RefCell<usize>,
    }

    impl LlmGenerator for FlakyLlm {
        fn generate(
            &self,
            _model: &str,
            _system: &str,
            _user: &str,
        ) -> Result<String, crate::ports::LlmError> {
            let mut calls = self.calls.borrow_mut();
            *calls += 1;
            if *calls == 1 {
                Ok(self.reply.clone())
            } else {
                Err(crate::ports::LlmError("boom".into()))
            }
        }
    }

    #[test]
    fn a_model_error_mid_chain_keeps_the_fixes_that_already_landed() {
        let service = service(Ok(spec()), green());
        let llm = FlakyLlm {
            reply: REWORDING.into(),
            calls: std::cell::RefCell::new(0),
        };
        // Two findings: the first call lands the full fix, the second
        // fails - the reworded draft still seeds the next pass.
        let mut prompter = ScriptedPrompter::answering(&[
            "",
            "Comma sums",
            "The user gets comma sums so that totals come from one input",
            CLEAN_CRITERION,
            "",
            "",
            "",
            "",
            "",
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        let told = |line: &str| {
            assert!(
                prompter.transcript.iter().any(|l| l == line),
                "missing {line:?} in transcript: {:#?}",
                prompter.transcript
            );
        };
        told("The model call failed (boom).");
        told(
            "The model reworded the draft. Each prompt shows its proposal - Enter \
             accepts it, or type your own wording.",
        );
        told("REQ-008 title [Comma separated numbers are summed] (Enter keeps it):");
    }

    #[test]
    fn an_unusable_rewording_reply_falls_back_to_hand_rewording() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok("Sure! Here is a better wording:".into()));
        let mut prompter = ScriptedPrompter::answering(&[
            "",
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            "",
            "",
            "",
            "",
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert!(
            prompter.transcript.iter().any(|l| l
                == "The model's rewording for finding 1 was unusable - it stays \
                          yours to fix."),
            "transcript: {:#?}",
            prompter.transcript
        );
        assert!(
            prompter
                .transcript
                .iter()
                .any(|l| l.starts_with("Reword the requirement to address each finding.")),
            "the manual instructions still print on fallback"
        );
    }

    #[test]
    fn a_model_error_during_rewording_falls_back_to_hand_rewording() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Err("boom".into()));
        let mut prompter = ScriptedPrompter::answering(&[
            "",
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            "",
            "",
            "",
            "",
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert!(
            prompter
                .transcript
                .iter()
                .any(|l| l == "The model call failed (boom)."),
            "transcript: {:#?}",
            prompter.transcript
        );
    }

    #[test]
    fn a_model_error_falls_back_to_manual_drafting() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Err("ollama is not reachable".into()));
        let mut prompter = ScriptedPrompter::answering(&[
            "sum numbers from a string",
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        let told = |fragment: &str| {
            assert!(
                prompter.transcript.iter().any(|l| l.contains(fragment)),
                "missing {fragment:?} in transcript: {:#?}",
                prompter.transcript
            );
        };
        told("The model call failed (ollama is not reachable).");
        told("drafting manually");
    }

    #[test]
    fn an_unusable_reply_falls_back_to_manual_drafting() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok("Sure! Here are the requirements:".into()));
        let mut prompter = ScriptedPrompter::answering(&[
            "sum numbers from a string",
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert!(
            prompter
                .transcript
                .iter()
                .any(|l| l.contains("The description gave no complete requirement")),
            "transcript: {:#?}",
            prompter.transcript
        );
    }

    #[test]
    fn a_single_proposal_seeds_the_wizard_without_a_selection_question() {
        let single = r#"[{"title": "Empty string returns zero",
            "story": "As a user, I want empty input to be 0 so that no input is a safe default.",
            "acceptanceCriteria": ["Given an empty string \"\", when add is called, then the result is 0"]}]"#;
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok(single.into()));
        let mut prompter =
            ScriptedPrompter::answering(&["empty input means zero", "", "", "", "", "y"]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert_eq!(report.title, "Empty string returns zero");
        assert!(
            !prompter
                .transcript
                .iter()
                .any(|l| l.contains("Which requirement first?")),
            "transcript: {:#?}",
            prompter.transcript
        );
        assert!(
            prompter
                .transcript
                .iter()
                .any(|l| l == "REQ-008 title [Empty string returns zero] (Enter keeps it):"),
            "transcript: {:#?}",
            prompter.transcript
        );
    }

    #[test]
    fn the_pick_selects_the_proposal_and_the_rest_are_noted_for_later() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok(PROPOSALS.into()));
        let mut prompter = ScriptedPrompter::answering(&[
            "sum numbers, empty means zero",
            "2",
            "",
            "",
            "",
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert_eq!(report.title, "Empty string returns zero");
        let told = |fragment: &str| {
            assert!(
                prompter.transcript.iter().any(|l| l.contains(fragment)),
                "missing {fragment:?} in transcript: {:#?}",
                prompter.transcript
            );
        };
        told("The description holds 2 requirement(s):");
        told("1. Comma separated numbers are summed");
        told("2. Empty string returns zero");
        told("Left for later runs: Comma separated numbers are summed.");
    }

    #[test]
    fn an_invalid_pick_reasks_until_a_number_from_the_list_arrives() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok(PROPOSALS.into()));
        let mut prompter = ScriptedPrompter::answering(&[
            "sum numbers, empty means zero",
            "9",
            "first",
            "2",
            "",
            "",
            "",
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert_eq!(report.title, "Empty string returns zero");
        assert_eq!(
            prompter
                .transcript
                .iter()
                .filter(|l| l.as_str() == "Pick a number between 1 and 2.")
                .count(),
            2,
            "transcript: {:#?}",
            prompter.transcript
        );
    }

    #[test]
    fn an_empty_pick_means_the_first_proposal() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok(PROPOSALS.into()));
        let mut prompter = ScriptedPrompter::answering(&[
            "sum numbers, empty means zero",
            "",
            "",
            "",
            "",
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap();
        assert!(report.staged, "report: {report:?}");
        assert_eq!(report.title, "Comma separated numbers are summed");
    }

    #[test]
    fn a_description_prompt_error_propagates() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok(PROPOSALS.into()));
        let mut prompter = ScriptedPrompter::answering(&[]);
        let error = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap_err();
        assert!(error.0.contains("script exhausted"), "error: {error:?}");
    }

    #[test]
    fn a_pick_prompt_error_propagates() {
        let service = service(Ok(spec()), green());
        let llm = FakeLlm(Ok(PROPOSALS.into()));
        let mut prompter = ScriptedPrompter::answering(&["sum numbers, empty means zero"]);
        let error = service
            .draft_assisted(&mut prompter, "test-model", &llm)
            .unwrap_err();
        assert!(error.0.contains("script exhausted"), "error: {error:?}");
    }

    #[test]
    fn a_declined_draft_stages_nothing() {
        let service = service(Ok(spec()), green());
        let mut prompter = ScriptedPrompter::answering(&[
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "n",
        ]);
        let report = service.draft(&mut prompter).unwrap();
        assert!(!report.staged);
        assert!(report.next_step.starts_with("Nothing was staged."));
        assert_eq!(service.store.content(SPEC_PATH).unwrap(), None);
    }

    #[test]
    fn drafting_stacks_on_a_previously_staged_spec() {
        let service = service(Ok(spec()), green());
        service
            .store
            .stage(
                SPEC_PATH,
                &serde_json::to_string(&Spec {
                    project: "Kata".into(),
                    description: None,
                    requirements: vec![requirement("REQ-011")],
                })
                .unwrap(),
                "earlier draft",
            )
            .unwrap();
        let mut prompter = ScriptedPrompter::answering(&[
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "y",
        ]);
        let report = service.draft(&mut prompter).unwrap();
        assert_eq!(report.id, "REQ-012");
    }

    #[test]
    fn an_exhausted_prompt_script_propagates_as_an_error() {
        let service = service(Ok(spec()), green());
        let mut prompter = ScriptedPrompter::answering(&["Comma sums"]);
        let error = service.draft(&mut prompter).unwrap_err();
        assert_eq!(
            error,
            ServiceError("input is not readable - script exhausted".into())
        );
    }

    #[test]
    fn a_prompt_error_on_a_rewording_pass_field_propagates() {
        let service = service(Ok(spec()), green());
        // The script ends right where the second pass re-asks the title.
        let mut prompter = ScriptedPrompter::answering(&[
            "Comma sums",
            "the calculator should handle commas quickly",
            "the result is 3",
            "",
        ]);
        let error = service.draft(&mut prompter).unwrap_err();
        assert_eq!(
            error,
            ServiceError("input is not readable - script exhausted".into())
        );
    }

    #[test]
    fn a_prompt_error_on_a_prior_criterion_propagates() {
        let service = service(Ok(spec()), green());
        // The script ends right where the second pass offers criterion 1.
        let mut prompter = ScriptedPrompter::answering(&[
            "Comma sums",
            "the calculator should handle commas quickly",
            "the result is 3",
            "",
            "",
            "",
        ]);
        let error = service.draft(&mut prompter).unwrap_err();
        assert_eq!(
            error,
            ServiceError("input is not readable - script exhausted".into())
        );
    }

    #[test]
    fn draft_propagates_a_failing_repository() {
        let service = service(Err(SpecError("spec: boom".into())), green());
        let mut prompter = ScriptedPrompter::default();
        assert_eq!(
            service.draft(&mut prompter).unwrap_err(),
            ServiceError("spec: boom".into())
        );
    }

    #[test]
    fn a_malformed_staged_spec_is_a_structured_error() {
        let service = service(Ok(spec()), green());
        service.store.stage(SPEC_PATH, "not json", "oops").unwrap();
        let mut prompter = ScriptedPrompter::default();
        let error = service.draft(&mut prompter).unwrap_err();
        assert!(
            error
                .0
                .starts_with("spec: staged requirements/requirements.json is not readable JSON -"),
            "got: {}",
            error.0
        );
    }

    #[test]
    fn the_first_draft_of_an_empty_spec_gets_req_001() {
        let service = service(
            Ok(Spec {
                project: "Kata".into(),
                description: None,
                requirements: vec![],
            }),
            green(),
        );
        let mut prompter = ScriptedPrompter::answering(&[
            "Comma sums",
            CLEAN_STORY,
            CLEAN_CRITERION,
            EDGE_CRITERION,
            "",
            "y",
        ]);
        assert_eq!(service.draft(&mut prompter).unwrap().id, "REQ-001");
    }

    #[test]
    fn mark_implemented_on_green_stages_the_status_flip_and_the_feature_file() {
        let service = service(Ok(spec()), green());
        let report = service.mark_implemented("REQ-001").unwrap();
        assert_eq!(
            report,
            MarkReport {
                id: "REQ-001".into(),
                status: "implemented".into(),
                staged: true,
                next_step: "Review with changes show, run bdd validate (it checks the \
                            @REQ-001 scenario exists), then bdd changes commit."
                    .into(),
            }
        );
        let staged = service.store.content(SPEC_PATH).unwrap().unwrap();
        let staged_spec: Spec = serde_json::from_str(&staged).unwrap();
        assert_eq!(staged_spec.requirements[0].status, "implemented");
        assert_eq!(
            staged_spec.requirements[0].feature_file.as_deref(),
            Some("features/calc.feature"),
            "the tagged feature is recorded so the spec validates"
        );
        assert_eq!(staged_spec.requirements[1].status, "pending");
    }

    #[test]
    fn mark_implemented_without_a_tagged_scenario_names_the_recovery_commands() {
        let service = service(Ok(spec()), green());
        let error = service.mark_implemented("REQ-007").unwrap_err();
        assert_eq!(
            error.0,
            "No scenario is tagged @REQ-007 - implemented requirements need an \
             executable scenario. Add one with bdd scenario add, apply it with \
             bdd changes commit, then mark REQ-007 implemented."
        );
        assert_eq!(service.store.content(SPEC_PATH).unwrap(), None);
    }

    #[test]
    fn a_rerun_backfills_the_feature_file_of_an_already_implemented_requirement() {
        // The broken-on-disk shape: implemented but no featureFile.
        let mut broken = spec();
        broken.requirements[0].status = "implemented".into();
        broken.requirements[0].feature_file = None;
        let service = service(Ok(broken), green());
        let report = service.mark_implemented("REQ-001").unwrap();
        assert!(report.staged);
        let staged: Spec =
            serde_json::from_str(&service.store.content(SPEC_PATH).unwrap().unwrap()).unwrap();
        assert_eq!(staged.requirements[0].status, "implemented");
        assert_eq!(
            staged.requirements[0].feature_file.as_deref(),
            Some("features/calc.feature")
        );
    }

    #[test]
    fn mark_implemented_is_refused_off_green() {
        for phase in [TddPhase::Start, TddPhase::Red, TddPhase::Refactor] {
            let service = service(Ok(spec()), FixedStateStore::holding(TddSnapshot::at(phase)));
            let error = service.mark_implemented("REQ-001").unwrap_err();
            assert_eq!(
                error.0,
                format!(
                    "Requirements are only marked implemented on GREEN (current \
                     phase: {phase}). Run the tests and make them pass first."
                )
            );
        }
    }

    #[test]
    fn mark_implemented_of_an_unknown_id_names_the_recovery_tool() {
        let service = service(Ok(spec()), green());
        assert_eq!(
            service.mark_implemented("REQ-999").unwrap_err(),
            ServiceError(
                "No requirement with id 'REQ-999'. Call list_requirements to see valid ids.".into()
            )
        );
    }

    #[test]
    fn mark_implemented_propagates_a_failing_state_store() {
        let service = service(
            Ok(spec()),
            FixedStateStore::failing(".bdd-state.json is not readable - boom"),
        );
        assert_eq!(
            service.mark_implemented("REQ-001").unwrap_err(),
            ServiceError(".bdd-state.json is not readable - boom".into())
        );
    }

    #[test]
    fn a_flag_draft_stages_the_next_id_without_a_tty() {
        let service = service(Ok(spec()), green());
        let report = service
            .draft_direct(
                "Newlines as delimiters",
                "As a calculator user, I want newlines to separate numbers in addition to commas so that multi-line input just works.",
                vec![
                    "Given the input \"1\\n2,3\", when add is called, then the result is 6".into(),
                    EDGE_CRITERION.into(),
                ],
            )
            .unwrap();
        assert_eq!(report.id, "REQ-008");
        assert!(report.staged);
        let listed = service.list_requirements().unwrap();
        assert!(
            listed.iter().any(|r| r.id == "REQ-008" && r.staged),
            "listed: {listed:?}"
        );
        assert!(listed.iter().any(|r| r.id == "REQ-001" && !r.staged));
    }

    #[test]
    fn a_duplicate_draft_warns_without_blocking() {
        let service = service(Ok(spec()), green());
        let report = service
            .draft_direct(
                "A title",
                CLEAN_STORY,
                vec![CLEAN_CRITERION.into(), EDGE_CRITERION.into()],
            )
            .unwrap();
        assert!(
            report.next_step.contains("looks similar to REQ-001"),
            "next step: {}",
            report.next_step
        );
    }

    #[test]
    fn set_feature_stages_a_path_rewrite() {
        let service = service(Ok(spec()), green());
        let report = service
            .set_feature("REQ-001", "src/test/resources/features/calc.feature")
            .unwrap();
        assert!(report.staged);
        assert_eq!(
            report.feature_file,
            "src/test/resources/features/calc.feature"
        );
        let noop = service
            .set_feature("REQ-001", "src/test/resources/features/calc.feature")
            .unwrap();
        assert!(!noop.staged);
    }

    #[test]
    fn reword_direct_replaces_an_existing_row() {
        let service = service(Ok(spec()), green());
        let report = service
            .reword_direct(
                "REQ-001",
                Some("Comma sums".into()),
                Some(CLEAN_STORY.into()),
                vec![CLEAN_CRITERION.into(), EDGE_CRITERION.into()],
            )
            .unwrap();
        assert_eq!(report.id, "REQ-001");
        let staged: Spec =
            serde_json::from_str(&service.store.content(SPEC_PATH).unwrap().unwrap()).unwrap();
        assert_eq!(staged.requirements[0].title, "Comma sums");
        assert_eq!(staged.requirements.len(), 2);
    }
}
