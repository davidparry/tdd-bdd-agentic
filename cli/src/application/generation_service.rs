//! Step and unit-test generation: discovery of undefined steps, and the
//! hybrid template + LLM generation flow. The deterministic template from
//! the domain always works; when a model is resolved its output is
//! preferred after validation, otherwise the template is used silently.
//! Everything generated lands in the staging area, never in working files.

use serde::Serialize;

use crate::application::assets::{
    find_missing_steps, find_requirement, load_effective_spec, production_path,
    production_type_name, unit_test_path,
};
use crate::application::generate_logged;
use crate::application::spec_service::ServiceError;
use crate::domain::generation::{
    append_unit_tests, looks_like_step_definitions, looks_like_unit_test, looks_like_unit_test_for,
    polish_prompt, step_definitions_template, steps_target_path, strip_code_fences,
    unit_test_target_path, unit_test_template,
};
use crate::domain::language::Language;
use crate::domain::steps::MissingStep;
use crate::ports::{ChangeStore, FeatureCatalog, LlmGenerator, SourceFiles, SpecRepository};

/// Reply of `bdd steps missing`.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct MissingStepsReport {
    pub language: String,
    pub framework: String,
    pub missing: Vec<MissingStep>,
    #[serde(rename = "nextStep")]
    pub next_step: String,
}

/// Reply of `bdd steps generate` and `bdd unittest generate`.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct GenerationReport {
    pub target: String,
    pub staged: bool,
    /// "template" for the deterministic output, "llm" when a model's
    /// polished version passed validation.
    pub source: String,
    pub summary: String,
    #[serde(rename = "nextStep")]
    pub next_step: String,
}

/// The resolved LLM, when one is available: model name + generator.
pub struct ResolvedLlm<L: LlmGenerator> {
    pub model: String,
    pub generator: L,
}

pub struct GenerationService<F, S, C, R, L>
where
    F: FeatureCatalog,
    S: SourceFiles,
    C: ChangeStore,
    R: SpecRepository,
    L: LlmGenerator,
{
    features: F,
    sources: S,
    store: C,
    spec: R,
    language: Language,
    llm: Option<ResolvedLlm<L>>,
}

impl<F, S, C, R, L> GenerationService<F, S, C, R, L>
where
    F: FeatureCatalog,
    S: SourceFiles,
    C: ChangeStore,
    R: SpecRepository,
    L: LlmGenerator,
{
    pub fn new(
        features: F,
        sources: S,
        store: C,
        spec: R,
        language: Language,
        llm: Option<ResolvedLlm<L>>,
    ) -> Self {
        Self {
            features,
            sources,
            store,
            spec,
            language,
            llm,
        }
    }

    /// Every feature step with no matching definition (step_definitions_find).
    pub fn steps_missing(&self) -> Result<MissingStepsReport, ServiceError> {
        let missing = find_missing_steps(&self.features, &self.sources, self.language)?;
        let next_step = if missing.is_empty() {
            "Every step has a definition. Run bdd test to execute the suite.".to_string()
        } else {
            format!(
                "{} step(s) have no definition. Run bdd steps generate to stage pending definitions for them.",
                missing.len()
            )
        };
        Ok(MissingStepsReport {
            language: self.language.display().to_string(),
            framework: self.language.bdd_framework().to_string(),
            missing,
            next_step,
        })
    }

    /// Stage pending step definitions for every undefined step
    /// (step_definition_create).
    pub fn steps_generate(&self) -> Result<GenerationReport, ServiceError> {
        let missing = find_missing_steps(&self.features, &self.sources, self.language)?;
        if missing.is_empty() {
            return Err(ServiceError(
                "Every step already has a definition - nothing to generate.".into(),
            ));
        }
        let template = step_definitions_template(self.language, &missing);
        let (content, source) = self.polish(&template, |code| {
            looks_like_step_definitions(self.language, code)
        });
        let target = steps_target_path(self.language).to_string();
        let summary = format!(
            "generate pending step definitions for {} missing step(s) ({source})",
            missing.len()
        );
        self.store
            .stage(&target, &content, &summary)
            .map_err(|e| ServiceError(e.0))?;
        Ok(GenerationReport {
            target,
            staged: true,
            source,
            summary,
            next_step: "Review with bdd changes show, apply with bdd changes commit, then run bdd test (expect RED)."
                .into(),
        })
    }

    /// Stage a failing unit test derived from one requirement's acceptance
    /// criteria (unit_test_create). When a brownfield test class already
    /// exists, the new methods are appended to it instead of writing a
    /// parallel `Req00NTest`.
    pub fn unittest_generate(&self, req_id: &str) -> Result<GenerationReport, ServiceError> {
        let spec = load_effective_spec(&self.spec, &self.store)?;
        let requirement = find_requirement(&spec, req_id)?;
        let sources = self
            .sources
            .sources(crate::domain::steps::source_extension(self.language))
            .map_err(|e| ServiceError(e.0))?;
        let target = unit_test_path(&sources, self.language, req_id);
        let conventional = unit_test_target_path(self.language, req_id);
        let existing = sources.iter().find(|file| file.path == target);
        let append = existing.is_some() && target != conventional;
        let template = match existing {
            Some(file) if append => append_unit_tests(&file.content, self.language, requirement),
            _ => unit_test_template(self.language, requirement),
        };
        let production = production_path(&sources, self.language, &spec.project);
        let production_type = production_type_name(&production);
        let package_line = existing.filter(|_| append).and_then(|file| {
            file.content
                .lines()
                .find(|line| line.starts_with("package "))
                .map(str::to_string)
        });
        let (content, source) = self.polish(&template, |code| {
            if append {
                looks_like_unit_test_for(self.language, code, production_type.as_deref())
                    && package_line
                        .as_deref()
                        .map(|pkg| code.contains(pkg.trim_end_matches(';')))
                        .unwrap_or(true)
            } else {
                looks_like_unit_test(self.language, code)
            }
        });
        let summary = format!(
            "generate failing unit test for {req_id} ({} criteria, {source})",
            requirement.acceptance_criteria.len()
        );
        self.store
            .stage(&target, &content, &summary)
            .map_err(|e| ServiceError(e.0))?;
        Ok(GenerationReport {
            target,
            staged: true,
            source,
            summary,
            next_step: "Review the assertions (they are yours to sharpen), apply with bdd changes commit, then run bdd test (expect RED)."
                .into(),
        })
    }

    /// The hybrid pass: prefer validated LLM output, fall back to the
    /// template silently on any failure.
    fn polish(&self, template: &str, valid: impl Fn(&str) -> bool) -> (String, String) {
        let Some(llm) = &self.llm else {
            return (template.to_string(), "template".into());
        };
        let prompt = polish_prompt(self.language, template);
        match generate_logged(&llm.generator, &llm.model, &prompt) {
            Ok(response) => {
                let code = strip_code_fences(&response);
                if valid(&code) {
                    (code, "llm".into())
                } else {
                    (template.to_string(), "template".into())
                }
            }
            Err(_) => (template.to_string(), "template".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::Spec;
    use crate::ports::StagedChange;
    use crate::test_support::{
        FailingSources, FakeLlm, FakeSources, InMemoryChangeStore, InMemoryFeatureCatalog,
        InMemorySpecRepository, calculator_catalog, calculator_spec,
    };

    fn service(
        sources: Vec<crate::ports::SourceFile>,
        llm: Option<ResolvedLlm<FakeLlm>>,
    ) -> GenerationService<
        InMemoryFeatureCatalog,
        FakeSources,
        InMemoryChangeStore,
        InMemorySpecRepository,
        FakeLlm,
    > {
        GenerationService::new(
            calculator_catalog(),
            FakeSources(sources),
            InMemoryChangeStore::default(),
            InMemorySpecRepository(Ok(calculator_spec())),
            Language::Java,
            llm,
        )
    }

    fn defined(patterns: &[&str]) -> Vec<crate::ports::SourceFile> {
        let body: String = patterns
            .iter()
            .map(|p| format!("@Given(\"{p}\")\npublic void step() {{}}\n"))
            .collect();
        vec![crate::ports::SourceFile {
            path: "src/test/java/Steps.java".into(),
            content: body,
        }]
    }

    fn staged(
        service: &GenerationService<
            InMemoryFeatureCatalog,
            FakeSources,
            InMemoryChangeStore,
            InMemorySpecRepository,
            FakeLlm,
        >,
    ) -> Vec<StagedChange> {
        service.store.changes().unwrap()
    }

    #[test]
    fn undefined_steps_are_reported_with_the_framework() {
        let report = service(vec![], None).steps_missing().unwrap();
        assert_eq!(report.language, "Java");
        assert_eq!(report.framework, "Cucumber-JVM");
        assert_eq!(report.missing.len(), 3);
        assert_eq!(
            report.next_step,
            "3 step(s) have no definition. Run bdd steps generate to stage pending definitions for them."
        );
    }

    #[test]
    fn fully_defined_features_report_nothing_missing() {
        let sources = defined(&[
            "a calculator",
            "add is called with {string}",
            "the result is {int}",
        ]);
        let report = service(sources, None).steps_missing().unwrap();
        assert_eq!(report.missing, vec![]);
        assert_eq!(
            report.next_step,
            "Every step has a definition. Run bdd test to execute the suite."
        );
    }

    #[test]
    fn generate_without_a_model_stages_the_template() {
        let service = service(vec![], None);
        let report = service.steps_generate().unwrap();
        assert_eq!(report.source, "template");
        assert_eq!(report.target, "src/test/java/GeneratedSteps.java");
        assert!(report.staged);
        let changes = staged(&service);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "src/test/java/GeneratedSteps.java");
        let content = service.store.content(&report.target).unwrap().unwrap();
        assert!(content.contains("@Given(\"a calculator\")"));
        assert!(content.contains("PendingException"));
    }

    #[test]
    fn generate_with_nothing_missing_is_refused() {
        let sources = defined(&[
            "a calculator",
            "add is called with {string}",
            "the result is {int}",
        ]);
        let error = service(sources, None).steps_generate().unwrap_err();
        assert_eq!(
            error.0,
            "Every step already has a definition - nothing to generate."
        );
    }

    #[test]
    fn validated_llm_output_replaces_the_template() {
        let reply = "public class GeneratedSteps {\n    @Given(\"a calculator\") public void polished() {}\n}";
        let llm = FakeLlm::replying(&format!("```java\n{reply}\n```"));
        let service = service(vec![], Some(llm));
        let report = service.steps_generate().unwrap();
        assert_eq!(report.source, "llm");
        let content = service.store.content(&report.target).unwrap().unwrap();
        assert_eq!(content, reply);
        let prompts = service.llm.as_ref().unwrap().generator.prompts.borrow();
        assert!(
            prompts[0].contains("Cucumber-JVM"),
            "prompt names the framework"
        );
        assert!(
            prompts[0].contains("@Given(\"a calculator\")"),
            "prompt carries the template"
        );
        assert!(
            prompts[0].contains("Java best practices to follow:")
                && prompts[0].contains("Package names are lowercase"),
            "prompt pins the language's best practices"
        );
    }

    #[test]
    fn invalid_llm_output_falls_back_to_the_template_silently() {
        let service = service(vec![], Some(FakeLlm::replying("I cannot help with that.")));
        let report = service.steps_generate().unwrap();
        assert_eq!(report.source, "template");
        let content = service.store.content(&report.target).unwrap().unwrap();
        assert!(content.contains("PendingException"));
    }

    #[test]
    fn an_llm_failure_falls_back_to_the_template_silently() {
        let service = service(vec![], Some(FakeLlm::failing()));
        let report = service.steps_generate().unwrap();
        assert_eq!(report.source, "template");
    }

    #[test]
    fn a_unit_test_is_staged_from_the_requirements_criteria() {
        let service = service(vec![], None);
        let report = service.unittest_generate("REQ-001").unwrap();
        assert_eq!(report.target, "src/test/java/Req001Test.java");
        assert_eq!(report.source, "template");
        assert!(report.summary.contains("1 criteria"));
        let content = service.store.content(&report.target).unwrap().unwrap();
        assert!(content.contains("Generated from REQ-001: Adds two numbers"));
        assert!(content.contains("fail(\"TODO: assert -"));
    }

    #[test]
    fn a_unit_test_for_an_unknown_requirement_is_refused() {
        let error = service(vec![], None)
            .unittest_generate("REQ-999")
            .unwrap_err();
        assert_eq!(
            error.0,
            "No requirement with id REQ-999. Call spec list to see valid ids."
        );
    }

    #[test]
    fn validated_llm_output_replaces_the_unit_test_template() {
        let llm = FakeLlm::replying("@Test void polished() {}");
        let service = service(vec![], Some(llm));
        let report = service.unittest_generate("REQ-001").unwrap();
        assert_eq!(report.source, "llm");
        let content = service.store.content(&report.target).unwrap().unwrap();
        assert_eq!(content, "@Test void polished() {}");
    }

    #[test]
    fn a_brownfield_unit_test_is_appended_not_written_in_parallel() {
        let sources = vec![crate::ports::SourceFile {
            path: "src/test/java/com/example/StringCalculatorTest.java".into(),
            content: "package com.example;\n\nclass StringCalculatorTest {\n}\n".into(),
        }];
        let service = service(sources, None);
        let report = service.unittest_generate("REQ-001").unwrap();
        assert_eq!(
            report.target,
            "src/test/java/com/example/StringCalculatorTest.java"
        );
        let content = service.store.content(&report.target).unwrap().unwrap();
        assert!(content.contains("package com.example;"));
        assert!(content.contains("class StringCalculatorTest"));
        assert!(content.contains("REQ-001"));
        assert!(content.contains("fail(\"TODO: assert -"));
        assert!(!content.contains("class Req001Test"));
    }

    #[test]
    fn llm_polish_that_renames_the_package_falls_back_to_the_template() {
        let sources = vec![crate::ports::SourceFile {
            path: "src/test/java/com/example/StringCalculatorTest.java".into(),
            content: "package com.example;\n\nclass StringCalculatorTest {\n    private final StringCalculator calculator = new StringCalculator();\n}\n".into(),
        }];
        let llm = FakeLlm::replying(
            "package com.wrong;\n@Test void two() { fail(\"TODO\"); new StringCalculator(); }",
        );
        let service = service(sources, Some(llm));
        let report = service.unittest_generate("REQ-001").unwrap();
        assert_eq!(report.source, "template");
        let content = service.store.content(&report.target).unwrap().unwrap();
        assert!(content.contains("package com.example;"));
        assert!(!content.contains("package com.wrong;"));
    }

    #[test]
    fn source_scan_failures_become_service_errors() {
        let service: GenerationService<_, _, InMemoryChangeStore, InMemorySpecRepository, FakeLlm> =
            GenerationService::new(
                calculator_catalog(),
                FailingSources,
                InMemoryChangeStore::default(),
                InMemorySpecRepository(Ok(Spec::default())),
                Language::Java,
                None,
            );
        assert_eq!(service.steps_missing().unwrap_err().0, "disk on fire");
    }
}
