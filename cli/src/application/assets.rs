//! The requirement/asset queries shared by step generation, the
//! implement preflight, and `bdd status`: looking a requirement up by
//! id, finding undefined steps, and surveying the assets an
//! implementation rests on.

use crate::application::spec_service::ServiceError;
use crate::domain::feature::FeatureDoc;
use crate::domain::generation::{
    ImplementAsset, implementation_target_path, steps_target_path, unit_test_target_path,
};
use crate::domain::language::Language;
use crate::domain::model::{Requirement, Spec};
use crate::domain::steps::{MissingStep, extract_patterns, find_missing, source_extension};
use crate::ports::{FeatureCatalog, SourceFiles};

/// The requirement with `req_id`, or the refusal naming the recovery
/// command.
pub(crate) fn find_requirement<'a>(
    spec: &'a Spec,
    req_id: &str,
) -> Result<&'a Requirement, ServiceError> {
    spec.requirements
        .iter()
        .find(|r| r.id == req_id)
        .ok_or_else(|| {
            ServiceError(format!(
                "No requirement with id {req_id}. Call spec list to see valid ids."
            ))
        })
}

/// Every feature step with no matching definition in the sources.
pub(crate) fn find_missing_steps(
    features: &impl FeatureCatalog,
    sources: &impl SourceFiles,
    language: Language,
) -> Result<Vec<MissingStep>, ServiceError> {
    let docs: Vec<FeatureDoc> = features
        .list()
        .map_err(|e| ServiceError(e.0))?
        .iter()
        .map(|summary| features.read(&summary.path))
        .collect::<Result<_, _>>()
        .map_err(|e| ServiceError(e.0))?;
    let patterns: Vec<String> = sources
        .sources(source_extension(language))
        .map_err(|e| ServiceError(e.0))?
        .iter()
        .flat_map(|file| extract_patterns(language, &file.content))
        .collect();
    Ok(find_missing(&docs, &patterns))
}

/// The first feature file carrying `tag` on the feature or any scenario
/// in it.
pub(crate) fn feature_tagged(
    features: &impl FeatureCatalog,
    tag: &str,
) -> Result<Option<String>, ServiceError> {
    for summary in features.list().map_err(|e| ServiceError(e.0))? {
        let doc = features
            .read(&summary.path)
            .map_err(|e| ServiceError(e.0))?;
        if doc.all_tags().iter().any(|t| t == tag) {
            return Ok(Some(doc.path));
        }
    }
    Ok(None)
}

/// Survey the assets a requirement's implementation rests on - the
/// tagged scenario, the step definitions, the unit test, the
/// production file - and the finding (naming the command to run)
/// for each one that is missing. Shared by the implement preflight
/// and `bdd status`.
pub(crate) fn asset_survey(
    features: &impl FeatureCatalog,
    sources: &impl SourceFiles,
    language: Language,
    req_id: &str,
    requirement: &Requirement,
    project: &str,
) -> Result<(Vec<ImplementAsset>, Vec<String>), ServiceError> {
    let mut assets = Vec::new();
    let mut findings = Vec::new();
    let tag = format!("@{req_id}");
    let tagged_feature = feature_tagged(features, &tag)?;
    if tagged_feature.is_none() {
        findings.push(format!(
            "No scenario is tagged {tag} - add one with bdd scenario add, \
             then bdd changes commit."
        ));
    }
    assets.push(ImplementAsset {
        role: format!("scenario tagged {tag}"),
        path: tagged_feature.clone().unwrap_or_else(|| {
            requirement
                .feature_file
                .clone()
                .unwrap_or_else(|| "features/*.feature".into())
        }),
        present: tagged_feature.is_some(),
    });

    let missing_steps = find_missing_steps(features, sources, language)?;
    if !missing_steps.is_empty() {
        findings.push(format!(
            "{} step(s) have no definition - run bdd steps generate, \
             then bdd changes commit.",
            missing_steps.len()
        ));
    }
    assets.push(ImplementAsset {
        role: "step definitions (every step defined)".into(),
        path: steps_target_path(language).to_string(),
        present: missing_steps.is_empty(),
    });

    let source_files = sources
        .sources(source_extension(language))
        .map_err(|e| ServiceError(e.0))?;
    let unit_test = unit_test_target_path(language, req_id);
    let unit_test_present = source_files.iter().any(|file| file.path == unit_test);
    if !unit_test_present {
        findings.push(format!(
            "The unit test {unit_test} does not exist - run bdd unittest \
             generate {req_id}, then bdd changes commit."
        ));
    }
    assets.push(ImplementAsset {
        role: "unit test".into(),
        path: unit_test,
        present: unit_test_present,
    });

    let production = implementation_target_path(language, project);
    assets.push(ImplementAsset {
        role: "production code (the attempt creates it when missing)".into(),
        path: production.clone(),
        present: source_files.iter().any(|file| file.path == production),
    });
    Ok((assets, findings))
}
