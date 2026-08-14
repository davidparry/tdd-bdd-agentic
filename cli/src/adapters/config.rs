//! TOML configuration adapter: the `[llm]` block of `.bdd-mcp.toml`
//! holds the persisted model choice and the provider endpoint.

use std::fs;
use std::path::PathBuf;

use crate::ports::{LlmError, ModelStore};

pub struct TomlModelStore {
    config_file: PathBuf,
}

impl TomlModelStore {
    pub fn new(config_file: PathBuf) -> Self {
        Self { config_file }
    }

    /// The configured provider endpoint, when one is set.
    pub fn endpoint(&self) -> Option<String> {
        self.llm_key("endpoint")
    }

    /// The configured generation timeout in seconds, when one is set.
    /// Large prompts on local models can outlast the default.
    pub fn timeout_seconds(&self) -> Option<u64> {
        let table = self.read_table()?;
        table
            .get("llm")
            .and_then(|llm| llm.get("timeout_seconds"))
            .and_then(toml::Value::as_integer)
            .and_then(|seconds| u64::try_from(seconds).ok())
    }

    fn llm_key(&self, key: &str) -> Option<String> {
        let table = self.read_table()?;
        table
            .get("llm")
            .and_then(|llm| llm.get(key))
            .and_then(|value| value.as_str())
            .map(String::from)
    }

    fn read_table(&self) -> Option<toml::Table> {
        fs::read_to_string(&self.config_file)
            .ok()?
            .parse::<toml::Table>()
            .ok()
    }
}

impl ModelStore for TomlModelStore {
    fn configured(&self) -> Option<String> {
        self.llm_key("model")
    }

    fn persist(&self, model: &str) -> Result<(), LlmError> {
        let mut table = self.read_table().unwrap_or_default();
        let llm = table
            .entry("llm".to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let llm_table = llm
            .as_table_mut()
            .ok_or_else(|| LlmError("config: [llm] is not a table".into()))?;
        llm_table.insert("model".to_string(), toml::Value::String(model.to_string()));
        let rendered = toml::to_string_pretty(&table).expect("a plain TOML table always renders");
        fs::write(&self.config_file, rendered).map_err(|e| {
            LlmError(format!(
                "config: cannot write {} - {e}",
                self.config_file.display()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &tempfile::TempDir) -> TomlModelStore {
        TomlModelStore::new(dir.path().join(".bdd-mcp.toml"))
    }

    #[test]
    fn a_missing_config_file_means_nothing_is_configured() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        assert_eq!(store.configured(), None);
        assert_eq!(store.endpoint(), None);
        assert_eq!(store.timeout_seconds(), None);
    }

    #[test]
    fn a_configured_timeout_is_read_and_junk_values_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".bdd-mcp.toml");
        fs::write(&path, "[llm]\ntimeout_seconds = 600\n").unwrap();
        assert_eq!(
            TomlModelStore::new(path.clone()).timeout_seconds(),
            Some(600)
        );
        fs::write(&path, "[llm]\ntimeout_seconds = \"soon\"\n").unwrap();
        assert_eq!(TomlModelStore::new(path.clone()).timeout_seconds(), None);
        fs::write(&path, "[llm]\ntimeout_seconds = -5\n").unwrap();
        assert_eq!(TomlModelStore::new(path).timeout_seconds(), None);
    }

    #[test]
    fn persist_then_read_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.persist("llama3:latest").unwrap();
        assert_eq!(store.configured(), Some("llama3:latest".to_string()));
    }

    #[test]
    fn persist_rejects_a_config_where_llm_is_not_a_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".bdd-mcp.toml");
        fs::write(&path, "llm = \"not a table\"\n").unwrap();
        let error = TomlModelStore::new(path).persist("qwen3:8b").unwrap_err();
        assert_eq!(error, LlmError("config: [llm] is not a table".into()));
    }

    #[test]
    fn persist_reports_an_unwritable_location() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-such-dir").join(".bdd-mcp.toml");
        let error = TomlModelStore::new(path).persist("qwen3:8b").unwrap_err();
        assert!(
            error.0.starts_with("config: cannot write"),
            "got: {}",
            error.0
        );
    }

    #[test]
    fn persist_preserves_unrelated_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".bdd-mcp.toml");
        fs::write(
            &path,
            "[llm]\nendpoint = \"http://box:11434\"\n\n[policy]\nstrict = true\n",
        )
        .unwrap();
        let store = TomlModelStore::new(path.clone());
        store.persist("qwen3:8b").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("endpoint"), "endpoint kept: {content}");
        assert!(content.contains("strict"), "policy kept: {content}");
        assert_eq!(store.configured(), Some("qwen3:8b".to_string()));
        assert_eq!(store.endpoint(), Some("http://box:11434".to_string()));
    }
}
