//! Filesystem implementations of the spec and feature-file ports.

use std::fs;
use std::path::PathBuf;

use crate::domain::model::Spec;
use crate::ports::{FeatureFiles, SpecError, SpecRepository};

/// Reads the requirements spec fresh on every call — the spec the user
/// just saved is the spec that gets validated.
pub struct FsSpecRepository {
    spec_file: PathBuf,
}

impl FsSpecRepository {
    pub fn new(spec_file: PathBuf) -> Self {
        Self { spec_file }
    }
}

impl SpecRepository for FsSpecRepository {
    fn load(&self) -> Result<Spec, SpecError> {
        let file_name = self
            .spec_file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.spec_file.display().to_string());
        let content = fs::read_to_string(&self.spec_file)
            .map_err(|e| SpecError(format!("spec: {file_name} is not readable - {e}")))?;
        serde_json::from_str(&content)
            .map_err(|e| SpecError(format!("spec: {file_name} is not readable JSON - {e}")))
    }
}

/// Answers feature-file questions relative to the workshop root.
pub struct FsFeatureFiles {
    root: PathBuf,
}

impl FsFeatureFiles {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl FeatureFiles for FsFeatureFiles {
    fn exists(&self, path: &str) -> bool {
        self.root.join(path).is_file()
    }

    fn has_tag(&self, path: &str, tag: &str) -> bool {
        fs::read_to_string(self.root.join(path))
            .map(|content| content.contains(tag))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    #[test]
    fn a_valid_spec_file_loads() {
        let root = temp_root();
        let spec_file = root.path().join("requirements.json");
        fs::write(
            &spec_file,
            r#"{"project":"Kata","requirements":[{"id":"REQ-001","title":"T",
                "status":"pending","story":"s","acceptanceCriteria":[]}]}"#,
        )
        .unwrap();
        let spec = FsSpecRepository::new(spec_file).load().unwrap();
        assert_eq!(spec.project, "Kata");
        assert_eq!(spec.requirements[0].id, "REQ-001");
    }

    #[test]
    fn a_missing_file_reports_not_readable() {
        let root = temp_root();
        let error = FsSpecRepository::new(root.path().join("requirements.json"))
            .load()
            .unwrap_err();
        assert!(
            error
                .0
                .starts_with("spec: requirements.json is not readable -")
        );
    }

    #[test]
    fn a_path_without_a_file_name_falls_back_to_the_full_path_in_the_error() {
        let error = FsSpecRepository::new(PathBuf::from("/"))
            .load()
            .unwrap_err();
        assert!(
            error.0.starts_with("spec: / is not readable -"),
            "got: {}",
            error.0
        );
    }

    #[test]
    fn broken_json_reports_not_readable_json() {
        let root = temp_root();
        let spec_file = root.path().join("requirements.json");
        fs::write(&spec_file, "{ nope").unwrap();
        let error = FsSpecRepository::new(spec_file).load().unwrap_err();
        assert!(
            error
                .0
                .starts_with("spec: requirements.json is not readable JSON -")
        );
    }

    #[test]
    fn feature_files_answer_existence_and_tags_relative_to_the_root() {
        let root = temp_root();
        let features_dir = root.path().join("features");
        fs::create_dir_all(&features_dir).unwrap();
        fs::write(features_dir.join("x.feature"), "@REQ-003\nScenario: s\n").unwrap();
        let files = FsFeatureFiles::new(root.path().to_path_buf());
        assert!(files.exists("features/x.feature"));
        assert!(!files.exists("features/missing.feature"));
        assert!(files.has_tag("features/x.feature", "@REQ-003"));
        assert!(!files.has_tag("features/x.feature", "@REQ-004"));
        assert!(!files.has_tag("features/missing.feature", "@REQ-003"));
    }
}
