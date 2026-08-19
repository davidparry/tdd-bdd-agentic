//! Staging overlays: feature catalogs and source trees that see
//! `.bdd-staged/` as if it were already committed. `bdd validate`
//! already overlays Gherkin; implement, status, and generation need
//! the same view so a just-staged scenario or test counts as present.

use crate::domain::feature::{self, FeatureDoc, FeatureSummary};
use crate::ports::{
    ChangeStore, FeatureCatalog, FeatureError, SourceError, SourceFile, SourceFiles,
};

pub struct OverlayCatalog<F, C> {
    inner: F,
    store: C,
}

impl<F: FeatureCatalog, C: ChangeStore> OverlayCatalog<F, C> {
    pub fn new(inner: F, store: C) -> Self {
        Self { inner, store }
    }
}

impl<F: FeatureCatalog, C: ChangeStore> FeatureCatalog for OverlayCatalog<F, C> {
    fn list(&self) -> Result<Vec<FeatureSummary>, FeatureError> {
        let mut summaries = self.inner.list()?;
        let changes = self.store.changes().map_err(|e| FeatureError(e.0))?;
        for change in changes.into_iter().filter(|c| c.path.ends_with(".feature")) {
            let Some(content) = self
                .store
                .content(&change.path)
                .map_err(|e| FeatureError(e.0))?
            else {
                continue;
            };
            let doc = feature::parse(&change.path, &content).map_err(FeatureError)?;
            if let Some(existing) = summaries.iter_mut().find(|s| s.path == change.path) {
                *existing = doc.summary();
            } else {
                summaries.push(doc.summary());
            }
        }
        Ok(summaries)
    }

    fn read(&self, path: &str) -> Result<FeatureDoc, FeatureError> {
        if let Ok(Some(content)) = self.store.content(path) {
            return feature::parse(path, &content).map_err(FeatureError);
        }
        self.inner.read(path)
    }

    fn exists(&self, path: &str) -> bool {
        matches!(self.store.content(path), Ok(Some(_))) || self.inner.exists(path)
    }
}

pub struct OverlaySources<S, C> {
    inner: S,
    store: C,
}

impl<S: SourceFiles, C: ChangeStore> OverlaySources<S, C> {
    pub fn new(inner: S, store: C) -> Self {
        Self { inner, store }
    }
}

impl<S: SourceFiles, C: ChangeStore> SourceFiles for OverlaySources<S, C> {
    fn sources(&self, extension: &str) -> Result<Vec<SourceFile>, SourceError> {
        let mut files = self.inner.sources(extension)?;
        let suffix = format!(".{extension}");
        let changes = self.store.changes().map_err(|e| SourceError(e.0))?;
        for change in changes.into_iter().filter(|c| c.path.ends_with(&suffix)) {
            let Some(content) = self
                .store
                .content(&change.path)
                .map_err(|e| SourceError(e.0))?
            else {
                continue;
            };
            if let Some(existing) = files.iter_mut().find(|f| f.path == change.path) {
                existing.content = content;
            } else {
                files.push(SourceFile {
                    path: change.path,
                    content,
                });
            }
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryChangeStore, InMemoryFeatureCatalog};
    use std::collections::HashMap;

    #[test]
    fn a_staged_feature_is_listed_and_readable() {
        let store = InMemoryChangeStore::default();
        store
            .stage(
                "features/new.feature",
                "Feature: New\n\n  @REQ-003\n  Scenario: s\n    Given a calculator\n",
                "add",
            )
            .unwrap();
        let overlay = OverlayCatalog::new(
            InMemoryFeatureCatalog {
                files: HashMap::new(),
            },
            store,
        );
        let list = overlay.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, "features/new.feature");
        assert!(overlay.exists("features/new.feature"));
        assert!(
            overlay
                .read("features/new.feature")
                .unwrap()
                .all_tags()
                .iter()
                .any(|t| t == "@REQ-003")
        );
    }

    #[test]
    fn a_staged_source_overrides_the_working_tree() {
        let store = InMemoryChangeStore::default();
        store
            .stage(
                "src/test/java/StringCalculatorTest.java",
                "@DisplayName(\"REQ-003\") @Test void two() {}",
                "generate",
            )
            .unwrap();
        let overlay = OverlaySources::new(
            crate::test_support::FakeSources(vec![crate::ports::SourceFile {
                path: "src/test/java/StringCalculatorTest.java".into(),
                content: "class StringCalculatorTest {}".into(),
            }]),
            store,
        );
        let files = overlay.sources("java").unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].content.contains("REQ-003"));
    }
}
