//! Ollama implementations of the [`ModelCatalog`] and [`LlmGenerator`]
//! ports: `GET /api/tags` for discovery and `POST /api/generate` for
//! generation, against the configured endpoint (default
//! `http://localhost:11434`). The HTTP shells are deliberately thin; the
//! JSON translations are pure functions so they are unit-testable
//! without a network.

use std::time::Duration;

use serde::Deserialize;

use crate::ports::{LlmError, LlmGenerator, ModelCatalog, ModelInfo};

pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434";

/// Local models chew on large prompts (an implementation attempt
/// carries the whole project) for minutes, not seconds.
pub const DEFAULT_GENERATION_TIMEOUT: Duration = Duration::from_secs(300);

/// The full story of a request failure. reqwest's Display keeps the
/// cause (connection refused, timed out, ...) in the source chain,
/// which would otherwise be dropped - a timeout then reads like the
/// provider is down.
fn describe(error: &reqwest::Error, timeout: Duration) -> String {
    if error.is_timeout() {
        return format!(
            "no reply within {}s - large prompts can outlast the timeout while \
             the model is still generating; set timeout_seconds under [llm] in \
             .bdd-mcp.toml to wait longer",
            timeout.as_secs()
        );
    }
    let mut messages = vec![error.to_string()];
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        messages.push(cause.to_string());
        source = cause.source();
    }
    messages.dedup();
    messages.join(" - ")
}

pub struct OllamaCatalog {
    endpoint: String,
    timeout: Duration,
}

impl OllamaCatalog {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            timeout: Duration::from_secs(5),
        }
    }
}

impl ModelCatalog for OllamaCatalog {
    fn models(&self) -> Result<Vec<ModelInfo>, LlmError> {
        let url = format!("{}/api/tags", self.endpoint.trim_end_matches('/'));
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .expect("a timeout-only HTTP client configuration cannot fail to build");
        let body = client
            .get(&url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(|response| response.text())
            .map_err(|e| {
                LlmError(format!(
                    "Ollama at {} - {}",
                    self.endpoint,
                    describe(&e, self.timeout)
                ))
            })?;
        parse_tags(&body)
    }
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<Tag>,
}

#[derive(Deserialize)]
struct Tag {
    name: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    modified_at: Option<String>,
}

/// Translates Ollama's `/api/tags` JSON into the port's model list.
pub fn parse_tags(body: &str) -> Result<Vec<ModelInfo>, LlmError> {
    let response: TagsResponse = serde_json::from_str(body)
        .map_err(|e| LlmError(format!("unexpected /api/tags response - {e}")))?;
    Ok(response
        .models
        .into_iter()
        .map(|tag| ModelInfo {
            name: tag.name,
            size_bytes: tag.size,
            modified_at: tag.modified_at,
        })
        .collect())
}

pub struct OllamaGenerator {
    endpoint: String,
    timeout: Duration,
}

impl OllamaGenerator {
    pub fn new(endpoint: String) -> Self {
        Self::with_timeout(endpoint, DEFAULT_GENERATION_TIMEOUT)
    }

    /// A custom generation timeout - `timeout_seconds` under `[llm]`
    /// in `.bdd-mcp.toml`.
    pub fn with_timeout(endpoint: String, timeout: Duration) -> Self {
        Self { endpoint, timeout }
    }
}

impl LlmGenerator for OllamaGenerator {
    fn generate(&self, model: &str, system: &str, user: &str) -> Result<String, LlmError> {
        let url = format!("{}/api/generate", self.endpoint.trim_end_matches('/'));
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .expect("a timeout-only HTTP client configuration cannot fail to build");
        let body = client
            .post(&url)
            .json(&serde_json::json!({
                "model": model,
                "system": system,
                "prompt": user,
                "stream": false,
            }))
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(|response| response.text())
            .map_err(|e| {
                LlmError(format!(
                    "Ollama at {} - {}",
                    self.endpoint,
                    describe(&e, self.timeout)
                ))
            })?;
        parse_generate(&body)
    }
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

/// Translates Ollama's `/api/generate` JSON into the completion text.
pub fn parse_generate(body: &str) -> Result<String, LlmError> {
    let response: GenerateResponse = serde_json::from_str(body)
        .map_err(|e| LlmError(format!("unexpected /api/generate response - {e}")))?;
    Ok(response.response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_json_translates_to_model_infos() {
        let body = r#"{"models":[
            {"name":"llama3:latest","size":4661224676,"modified_at":"2026-08-01T10:00:00Z"},
            {"name":"qwen3:8b"}
        ]}"#;
        let models = parse_tags(body).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama3:latest");
        assert_eq!(models[0].size_bytes, Some(4661224676));
        assert_eq!(models[1].name, "qwen3:8b");
        assert_eq!(models[1].size_bytes, None);
    }

    #[test]
    fn an_empty_models_array_is_an_empty_list_not_an_error() {
        assert!(parse_tags(r#"{"models":[]}"#).unwrap().is_empty());
        assert!(parse_tags("{}").unwrap().is_empty());
    }

    #[test]
    fn malformed_json_is_a_structured_error() {
        let error = parse_tags("nope").unwrap_err();
        assert!(error.0.starts_with("unexpected /api/tags response -"));
    }

    #[test]
    fn a_reachable_endpoint_returns_the_parsed_models() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = r#"{"models":[{"name":"llama3:latest"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let catalog = OllamaCatalog::new(format!("http://127.0.0.1:{port}/"));
        let models = catalog.models().unwrap();
        server.join().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "llama3:latest");
    }

    #[test]
    fn generate_json_translates_to_the_completion_text() {
        let body = r#"{"model":"llama3","response":"Given('a', ...)","done":true}"#;
        assert_eq!(parse_generate(body).unwrap(), "Given('a', ...)");
    }

    #[test]
    fn malformed_generate_json_is_a_structured_error() {
        let error = parse_generate("{}").unwrap_err();
        assert!(error.0.starts_with("unexpected /api/generate response -"));
    }

    #[test]
    fn a_reachable_endpoint_gets_both_prompts_and_returns_the_generated_text() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap();
            let body = r#"{"response":"generated code"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            String::from_utf8_lossy(&request[..read]).to_string()
        });
        let generator = OllamaGenerator::new(format!("http://127.0.0.1:{port}/"));
        let text = generator
            .generate("llama3", "you write steps", "write steps")
            .unwrap();
        let request = server.join().unwrap();
        assert_eq!(text, "generated code");
        assert!(
            request.contains(r#""system":"you write steps""#),
            "the system prompt travels in its own field: {request}"
        );
        assert!(
            request.contains(r#""prompt":"write steps""#),
            "the user prompt is the generation prompt: {request}"
        );
    }

    #[test]
    fn an_unreachable_generate_endpoint_reports_the_endpoint() {
        let generator = OllamaGenerator {
            endpoint: "http://127.0.0.1:9".into(),
            timeout: Duration::from_millis(300),
        };
        let error = generator
            .generate("llama3", "system", "prompt")
            .unwrap_err();
        assert!(error.0.contains("http://127.0.0.1:9"), "got: {}", error.0);
    }

    #[test]
    fn an_unreachable_endpoint_reports_the_endpoint_in_the_error() {
        // Port 9 (discard) is reliably closed/unroutable for a fast failure.
        let catalog = OllamaCatalog {
            endpoint: "http://127.0.0.1:9".into(),
            timeout: Duration::from_millis(300),
        };
        let error = catalog.models().unwrap_err();
        assert!(error.0.contains("http://127.0.0.1:9"), "got: {}", error.0);
    }

    #[test]
    fn a_connection_failure_reports_the_underlying_cause_not_just_the_wrapper() {
        let generator = OllamaGenerator {
            endpoint: "http://127.0.0.1:9".into(),
            timeout: Duration::from_millis(300),
        };
        let error = generator
            .generate("llama3", "system", "prompt")
            .unwrap_err();
        // The cause chain (e.g. "Connection refused") must survive, not
        // just reqwest's generic "error sending request" wrapper.
        assert!(
            error.0.matches(" - ").count() >= 2 || error.0.contains("refused"),
            "got: {}",
            error.0
        );
    }

    #[test]
    fn a_slow_reply_is_named_a_timeout_with_the_configured_budget() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            // Reply only after the client's timeout has expired.
            std::thread::sleep(Duration::from_millis(600));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
        });
        let generator = OllamaGenerator {
            endpoint: format!("http://127.0.0.1:{port}"),
            timeout: Duration::from_millis(200),
        };
        let error = generator
            .generate("llama3", "system", "prompt")
            .unwrap_err();
        server.join().unwrap();
        assert!(error.0.contains("no reply within 0s"), "got: {}", error.0);
        assert!(error.0.contains("timeout_seconds"), "got: {}", error.0);
    }

    #[test]
    fn the_default_generation_timeout_allows_long_completions() {
        assert_eq!(DEFAULT_GENERATION_TIMEOUT, Duration::from_secs(300));
    }
}
