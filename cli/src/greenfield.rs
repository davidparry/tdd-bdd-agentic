//! `bdd greenfield`: the orchestrated loop from an empty directory to an
//! implemented requirement. Exactly two human gates shape the run — the
//! spec wording approval (inside `spec draft`) and the generated-test
//! review before anything is committed. Everything else is derived from
//! the approved spec. Like [`crate::mcp`], this is a delivery module: it
//! wires the same application services, so it may name concrete adapters.
//! The prompter, runner factory, and LLM are injected so the whole
//! orchestration is testable with fakes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::adapters::fs_project::FsProjectFiles;
use crate::adapters::fs_scaffold::FsScaffoldWriter;
use crate::adapters::fs_sources::FsSourceFiles;
use crate::adapters::fs_spec::{FsFeatureFiles, FsSpecRepository};
use crate::adapters::fs_staging::FsChangeStore;
use crate::adapters::fs_state::FsStateStore;
use crate::adapters::gherkin_features::GherkinFeatureCatalog;
use crate::adapters::runners::detect_runner;
use crate::application::change_service::ChangeService;
use crate::application::generation_service::{GenerationService, ResolvedLlm};
use crate::application::implement_service::ImplementService;
use crate::application::init_service::InitService;
use crate::application::scenario_service::ScenarioService;
use crate::application::spec_mutation_service::SpecMutationService;
use crate::application::tdd_service::{TddError, TddService, TestReport};
use crate::domain::language::Language;
use crate::domain::language::detect_languages;
use crate::domain::model::Spec;
use crate::domain::scaffold::slug;
use crate::domain::steps::criterion_to_steps;
use crate::domain::tdd::ImplementAttempt;
use crate::ports::{
    ChangeStore as _, LlmError, LlmGenerator, PromptError, Prompter, TestFilter, TestRunner,
};
use crate::workspace::SPEC_PATH;

/// Picks the test runner for a project root, or explains why none fits.
pub type RunnerFactory = Arc<dyn Fn(&Path) -> Result<Box<dyn TestRunner>, String> + Send + Sync>;

/// [`LlmGenerator`] over a shared trait object, so the orchestrator can
/// carry whichever generator the composition root resolved.
#[derive(Clone)]
pub struct DynLlm(pub Arc<dyn LlmGenerator + Send + Sync>);

impl LlmGenerator for DynLlm {
    fn generate(&self, model: &str, system: &str, user: &str) -> Result<String, LlmError> {
        self.0.generate(model, system, user)
    }
}

/// Where the run ended, and what the human does next.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct GreenfieldReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    pub completed: bool,
    #[serde(rename = "nextStep")]
    pub next_step: String,
}

pub struct Greenfield {
    root: PathBuf,
    runner_factory: RunnerFactory,
    llm: Option<(String, DynLlm)>,
}

/// Parse a human answer into a supported language.
pub fn parse_language(answer: &str) -> Option<Language> {
    match answer.trim().to_lowercase().as_str() {
        "java" => Some(Language::Java),
        "javascript" | "js" => Some(Language::JavaScript),
        "typescript" | "ts" => Some(Language::TypeScript),
        "dotnet" | ".net" | "csharp" | "c#" => Some(Language::DotNet),
        "rust" => Some(Language::Rust),
        _ => None,
    }
}

/// Ask for a supported language until the answer parses - the one
/// prompt loop shared by `bdd init` and the greenfield scaffold step.
pub fn prompt_language(prompter: &mut dyn Prompter) -> Result<Language, PromptError> {
    loop {
        let answer = prompter
            .ask("Language for the new project (java, javascript, typescript, dotnet, rust):")?;
        match parse_language(&answer) {
            Some(language) => return Ok(language),
            None => prompter.warn("Unrecognized language - pick one of the five listed."),
        }
    }
}

impl Greenfield {
    pub fn new(root: PathBuf, llm: Option<(String, DynLlm)>) -> Self {
        Self::with_runner_factory(root, Arc::new(detect_runner), llm)
    }

    /// Constructor for tests: a scripted runner factory replaces real
    /// build-tool execution.
    pub fn with_runner_factory(
        root: PathBuf,
        runner_factory: RunnerFactory,
        llm: Option<(String, DynLlm)>,
    ) -> Self {
        Self {
            root,
            runner_factory,
            llm,
        }
    }

    pub fn run(&self, prompter: &mut dyn Prompter) -> Result<GreenfieldReport, String> {
        prompter.tell(
            "Greenfield loop: approved spec -> tagged scenario -> failing tests -> \
             implement -> GREEN -> refactor -> mark implemented.",
        );
        match &self.llm {
            Some((model, _)) => prompter.tell(&format!(
                "Generation uses model {model}; templates are the fallback."
            )),
            None => prompter.tell(
                "No LLM model resolved - deterministic templates will be used for generation.",
            ),
        }

        let language = self.ensure_project(prompter)?;
        prompter.tell(&format!(
            "Project language: {} ({}).",
            language.display(),
            language.bdd_framework()
        ));

        // Gate 1 (inside draft): the human words the spec and approves it.
        // With a model, drafting starts from a plain-words description the
        // model splits into requirement proposals the wizard walks through.
        let mutation = self.mutation_service();
        let draft = match &self.llm {
            Some((model, llm)) => mutation.draft_assisted(prompter, model, llm),
            None => mutation.draft(prompter),
        }
        .map_err(|e| e.0)?;
        if !draft.staged {
            return Ok(GreenfieldReport {
                requirement: None,
                feature: None,
                phase: None,
                completed: false,
                next_step:
                    "Nothing was staged. Run bdd greenfield again when the wording is ready.".into(),
            });
        }
        self.commit()?;
        prompter.tell(&format!("{} committed to the spec.", draft.id));

        let feature_path = self.author_scenarios(prompter, &draft.id, &draft.title)?;
        self.commit()?;
        prompter.tell(&format!("Scenarios committed to {feature_path}."));

        // Generation into staging, then gate 2: review before commit.
        let generation = self.generation_service(language);
        let implement = self.implement_service(language);
        let missing = generation.steps_missing().map_err(|e| e.0)?;
        if !missing.missing.is_empty() {
            let work = prompter.working("Generating step definitions - working");
            let report = generation.steps_generate().map_err(|e| e.0)?;
            drop(work);
            prompter.tell(&format!("Staged {} ({}).", report.target, report.source));
        }
        let work = prompter.working(&format!(
            "Generating the unit test for {} - working",
            draft.id
        ));
        let unit_test = generation.unittest_generate(&draft.id).map_err(|e| e.0)?;
        drop(work);
        prompter.tell(&format!(
            "Staged {} ({}).",
            unit_test.target, unit_test.source
        ));
        if let Some(content) = self
            .change_store()
            .content(&unit_test.target)
            .map_err(|e| e.0)?
        {
            prompter.tell("Generated unit test (the assertions are yours to sharpen):");
            prompter.tell(&content);
        }
        if !prompter
            .confirm("Commit the generated tests and step definitions?")
            .map_err(|e| e.0)?
        {
            self.change_service().discard().map_err(|e| e.0)?;
            return Ok(GreenfieldReport {
                requirement: Some(draft.id),
                feature: Some(feature_path),
                phase: None,
                completed: false,
                next_step: "Generation was discarded. Author the tests by hand or rerun \
                            bdd greenfield."
                    .into(),
            });
        }
        self.commit()?;

        // Execution only when the runtime is present; authoring is done
        // either way.
        let runner = match (self.runner_factory)(&self.root) {
            Ok(runner) => runner,
            Err(message) => {
                prompter.warn(&message);
                return Ok(self.authoring_done(draft.id, feature_path));
            }
        };
        let tdd = self.tdd_service();
        let Some(mut report) = self.try_run(&tdd, runner.as_ref(), prompter)? else {
            return Ok(self.authoring_done(draft.id, feature_path));
        };

        while report.phase != "GREEN" {
            let question = if self.llm.is_some() {
                "Press Enter to let the model attempt the implementation and rerun \
                 the tests, enter a number to attempt up to that many times without \
                 asking again, or type stop to pause here:"
            } else {
                "Implement the production code now. Press Enter to run the tests \
                 again, or type stop to pause here:"
            };
            let answer = prompter.ask(question).map_err(|e| e.0)?;
            if answer.eq_ignore_ascii_case("stop") {
                let next_step = format!(
                    "Paused on RED. Implement by hand or run bdd implement {}, \
                     then bdd test until GREEN, bdd refactor, and bdd spec \
                     mark-implemented.",
                    draft.id
                );
                return Ok(GreenfieldReport {
                    requirement: Some(draft.id),
                    feature: Some(feature_path),
                    phase: Some(report.phase),
                    completed: false,
                    next_step,
                });
            }
            // Without a model a number cannot buy extra attempts - the
            // developer is the implementation, so every rerun is asked for.
            let budget = if self.llm.is_some() {
                attempt_budget(&answer)
            } else {
                1
            };
            for attempt in 1..=budget {
                if budget > 1 {
                    prompter.tell(&format!("Attempt {attempt} of {budget}."));
                }
                if self.llm.is_some() {
                    self.attempt_implementation(prompter, &implement, &tdd, &draft.id)?;
                }
                report = match self.try_run(&tdd, runner.as_ref(), prompter)? {
                    Some(report) => report,
                    None => return Ok(self.authoring_done(draft.id, feature_path)),
                };
                if report.phase == "GREEN" {
                    break;
                }
            }
        }

        if prompter
            .confirm("Green bar. Start a refactor step before closing the loop?")
            .map_err(|e| e.0)?
        {
            let note = prompter
                .ask("What do you intend to refactor and why?")
                .map_err(|e| e.0)?;
            tdd.refactor(Some(&note)).map_err(tdd_message)?;
            report = self
                .try_run(&tdd, runner.as_ref(), prompter)?
                .ok_or_else(|| "the runtime disappeared mid-loop".to_string())?;
            if report.phase != "GREEN" {
                return Ok(GreenfieldReport {
                    requirement: Some(draft.id),
                    feature: Some(feature_path),
                    phase: Some(report.phase),
                    completed: false,
                    next_step: "The refactor broke the bar. Make the tests pass again, \
                                then bdd spec mark-implemented."
                        .into(),
                });
            }
        }

        let work = prompter.working("Saving status - working");
        self.mutation_service()
            .mark_implemented(&draft.id)
            .map_err(|e| e.0)?;
        self.commit()?;
        drop(work);
        prompter.tell(&format!("{} is implemented. Loop closed.", draft.id));
        Ok(GreenfieldReport {
            requirement: Some(draft.id),
            feature: Some(feature_path),
            phase: Some("GREEN".into()),
            completed: true,
            next_step: "The next requirement is waiting. Type greenfield to continue, \
                        or spec list."
                .into(),
        })
    }

    /// Detect the project language, scaffolding a new project first when
    /// the directory has no marker files.
    fn ensure_project(&self, prompter: &mut dyn Prompter) -> Result<Language, String> {
        let files = FsProjectFiles::new(self.root.clone());
        if let Some(language) = detect_languages(&files).first().copied() {
            return Ok(language);
        }
        prompter.tell("No project detected - scaffolding a new one.");
        let language = prompt_language(prompter).map_err(|e| e.0)?;
        let name = prompter.ask("Project name:").map_err(|e| e.0)?;
        let report = InitService::new(FsScaffoldWriter::new(self.root.clone()))
            .init(language, &name)
            .map_err(|e| e.0)?;
        prompter.tell(&format!(
            "Scaffolded {} files for {} ({}).",
            report.created.len(),
            report.language,
            report.framework
        ));
        Ok(language)
    }

    /// Turn the drafted requirement's criteria into a tagged feature file.
    fn author_scenarios(
        &self,
        prompter: &mut dyn Prompter,
        req_id: &str,
        title: &str,
    ) -> Result<String, String> {
        let feature_path = format!("features/{}.feature", slug(title));
        let spec: Spec = {
            let repository = FsSpecRepository::new(self.root.join(SPEC_PATH));
            crate::ports::SpecRepository::load(&repository).map_err(|e| e.0)?
        };
        let requirement = spec
            .requirements
            .iter()
            .find(|r| r.id == req_id)
            .ok_or_else(|| format!("{req_id} disappeared from the committed spec"))?;
        let scenarios = self.scenario_service();
        scenarios
            .create_feature(&feature_path, title)
            .map_err(|e| e.0)?;
        for (index, criterion) in requirement.acceptance_criteria.iter().enumerate() {
            let Some(steps) = criterion_to_steps(criterion) else {
                prompter.warn(&format!(
                    "Skipping criterion (not Given/when/then shaped): {criterion}"
                ));
                continue;
            };
            let name = format!("{} case {}", title, index + 1);
            scenarios
                .add_scenario(&feature_path, req_id, &name, steps)
                .map_err(|e| e.0)?;
        }
        Ok(feature_path)
    }

    fn authoring_done(&self, requirement: String, feature: String) -> GreenfieldReport {
        GreenfieldReport {
            requirement: Some(requirement),
            feature: Some(feature),
            phase: None,
            completed: false,
            next_step: "Authoring is complete. Install the runtime, then bdd test \
                        (expect RED), implement, and close the loop."
                .into(),
        }
    }

    /// Run the tests and narrate the outcome. `Ok(None)` means the
    /// runtime is missing: execution stops but authoring stands.
    fn try_run(
        &self,
        tdd: &TddService<FsStateStore>,
        runner: &dyn TestRunner,
        prompter: &mut dyn Prompter,
    ) -> Result<Option<TestReport>, String> {
        let work = prompter.working("Running the tests - working");
        let outcome = tdd.run_tests(runner, &TestFilter::default());
        drop(work);
        match outcome {
            Ok(report) => {
                prompter.tell(&format!(
                    "{}: {} tests, {} failures, {} errors.",
                    report.phase, report.tests, report.failures, report.errors
                ));
                for detail in &report.failure_details {
                    prompter.tell(&format!("  - {detail}"));
                }
                Ok(Some(report))
            }
            Err(TddError::RuntimeMissing { runtime, hint }) => {
                prompter.warn(&format!("Runtime missing ({runtime}): {hint}"));
                Ok(None)
            }
            Err(TddError::Other(message)) => Err(message),
        }
    }

    /// Ask the model to make the failing tests pass and commit whatever it
    /// staged. The brief carries the persisted failure details (stack
    /// traces included), prior attempts on this requirement, and only the
    /// three latest dated state entries. The attempt is logged so the next
    /// one learns from it. A model failure is narrated, not fatal - the
    /// developer can still implement by hand and press Enter again.
    fn attempt_implementation(
        &self,
        prompter: &mut dyn Prompter,
        implement: &ImplementService<
            GherkinFeatureCatalog,
            FsSourceFiles,
            FsChangeStore,
            FsSpecRepository,
            DynLlm,
        >,
        tdd: &TddService<FsStateStore>,
        req_id: &str,
    ) -> Result<(), String> {
        let brief = tdd.implementation_brief(req_id).map_err(tdd_message)?;
        let work = prompter.working("Generating an implementation attempt - working");
        let outcome = implement.generate(req_id, &brief.failures, &brief.history, &brief.states);
        drop(work);
        match outcome {
            Ok(attempt) => {
                for target in &attempt.targets {
                    // The complete path: targets are project-relative,
                    // but the reader may be anywhere on the machine.
                    let full = std::path::absolute(self.root.join(target))
                        .unwrap_or_else(|_| self.root.join(target));
                    prompter.tell(&format!("Updated {} (llm).", full.display()));
                }
                if let Some(warning) = &attempt.warning {
                    prompter.warn(warning);
                }
                tdd.record_attempt(ImplementAttempt {
                    requirement: req_id.to_string(),
                    targets: attempt.targets.clone(),
                    failures: brief.failures,
                    ..Default::default()
                })
                .map_err(tdd_message)?;
                self.commit()?;
            }
            Err(error) => prompter.warn(&format!("{} Implement by hand instead.", error.0)),
        }
        Ok(())
    }

    fn commit(&self) -> Result<(), String> {
        self.change_service().commit().map_err(|e| e.0)?;
        Ok(())
    }

    fn change_store(&self) -> FsChangeStore {
        FsChangeStore::new(self.root.clone())
    }

    fn change_service(&self) -> ChangeService<FsChangeStore, FsSpecRepository, FsFeatureFiles> {
        ChangeService::new(
            self.change_store(),
            FsSpecRepository::new(self.root.join(SPEC_PATH)),
            FsFeatureFiles::new(self.root.clone()),
            SPEC_PATH.into(),
        )
    }

    fn mutation_service(
        &self,
    ) -> SpecMutationService<
        FsSpecRepository,
        FsFeatureFiles,
        GherkinFeatureCatalog,
        FsChangeStore,
        FsStateStore,
    > {
        SpecMutationService::new(
            FsSpecRepository::new(self.root.join(SPEC_PATH)),
            FsFeatureFiles::new(self.root.clone()),
            GherkinFeatureCatalog::new(self.root.clone()),
            self.change_store(),
            FsStateStore::new(self.root.clone()),
            SPEC_PATH.into(),
        )
    }

    fn scenario_service(&self) -> ScenarioService<FsChangeStore, GherkinFeatureCatalog> {
        ScenarioService::new(
            self.change_store(),
            GherkinFeatureCatalog::new(self.root.clone()),
        )
    }

    fn generation_service(
        &self,
        language: Language,
    ) -> GenerationService<
        GherkinFeatureCatalog,
        FsSourceFiles,
        FsChangeStore,
        FsSpecRepository,
        DynLlm,
    > {
        GenerationService::new(
            GherkinFeatureCatalog::new(self.root.clone()),
            FsSourceFiles::new(self.root.clone()),
            self.change_store(),
            FsSpecRepository::new(self.root.join(SPEC_PATH)),
            language,
            self.llm
                .clone()
                .map(|(model, generator)| ResolvedLlm { model, generator }),
        )
    }

    fn implement_service(
        &self,
        language: Language,
    ) -> ImplementService<
        GherkinFeatureCatalog,
        FsSourceFiles,
        FsChangeStore,
        FsSpecRepository,
        DynLlm,
    > {
        ImplementService::new(
            GherkinFeatureCatalog::new(self.root.clone()),
            FsSourceFiles::new(self.root.clone()),
            self.change_store(),
            FsSpecRepository::new(self.root.join(SPEC_PATH)),
            language,
            self.llm
                .clone()
                .map(|(model, generator)| ResolvedLlm { model, generator }),
        )
    }

    fn tdd_service(&self) -> TddService<FsStateStore> {
        TddService::new(FsStateStore::new(self.root.clone()))
    }
}

/// The RED prompt's answer, read as an attempt budget: Enter buys a
/// single attempt, a positive number buys up to that many attempts
/// without asking again, and anything unreadable stays a single
/// attempt - exactly what Enter would have done.
fn attempt_budget(answer: &str) -> u32 {
    answer.parse::<u32>().ok().filter(|n| *n > 0).unwrap_or(1)
}

/// Flatten a TDD error into the message the human sees.
fn tdd_message(error: TddError) -> String {
    match error {
        TddError::Other(message) => message,
        TddError::RuntimeMissing { hint, .. } => hint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_answers_parse_with_aliases() {
        assert_eq!(parse_language("Java"), Some(Language::Java));
        assert_eq!(parse_language("js"), Some(Language::JavaScript));
        assert_eq!(parse_language("TS"), Some(Language::TypeScript));
        assert_eq!(parse_language(".NET"), Some(Language::DotNet));
        assert_eq!(parse_language("c#"), Some(Language::DotNet));
        assert_eq!(parse_language(" rust "), Some(Language::Rust));
        assert_eq!(parse_language("cobol"), None);
    }

    #[test]
    fn red_prompt_answers_parse_into_an_attempt_budget() {
        assert_eq!(attempt_budget(""), 1, "Enter is a single attempt");
        assert_eq!(attempt_budget("5"), 5);
        assert_eq!(attempt_budget("1"), 1);
        assert_eq!(attempt_budget("0"), 1, "zero cannot mean no attempt");
        assert_eq!(attempt_budget("-3"), 1);
        assert_eq!(attempt_budget("five"), 1, "junk behaves like Enter");
    }

    #[test]
    fn tdd_errors_flatten_to_their_human_message() {
        assert_eq!(tdd_message(TddError::Other("boom".into())), "boom");
        assert_eq!(
            tdd_message(TddError::RuntimeMissing {
                runtime: "JDK".into(),
                hint: "Install a JDK.".into(),
            }),
            "Install a JDK."
        );
    }

    struct EchoLlm;

    impl LlmGenerator for EchoLlm {
        fn generate(&self, model: &str, system: &str, user: &str) -> Result<String, LlmError> {
            Ok(format!("{model}:{system}:{user}"))
        }
    }

    #[test]
    fn a_dyn_llm_delegates_to_the_wrapped_generator() {
        let llm = DynLlm(Arc::new(EchoLlm));
        assert_eq!(llm.generate("m", "s", "p"), Ok("m:s:p".into()));
    }

    #[test]
    fn the_default_constructor_wires_the_real_runner_detection() {
        let dir = tempfile::tempdir().unwrap();
        let orchestrator = Greenfield::new(dir.path().to_path_buf(), None);
        // An empty directory has no build markers, so detection refuses.
        assert!((orchestrator.runner_factory)(dir.path()).is_err());
    }
}
