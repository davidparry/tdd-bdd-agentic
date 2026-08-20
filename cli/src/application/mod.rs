//! Use-case services. Each service is composed from domain logic and
//! [`crate::ports`] traits through constructor injection; no service names
//! a concrete adapter.

pub(crate) mod assets;
pub mod change_service;
pub mod command_service;
pub mod generation_service;
pub mod implement_service;
pub mod init_service;
pub mod inspect_service;
pub mod model_service;
pub mod scenario_service;
pub mod spec_mutation_service;
pub mod spec_service;
pub mod status_service;
pub mod tdd_service;

use crate::domain::prompts::RenderedPrompt;
use crate::ports::{LlmError, LlmGenerator};

/// One fully logged LLM round trip, used by every service that calls a
/// model. The complete system and user prompts go to the debug log
/// before the call and the entire reply (or the failure) after it, each
/// line tagged with the prompt-catalog section that rendered the message
/// (e.g. `[proposal]`, `[rewording]`), so the log names the template
/// behind every exchange.
pub(crate) fn generate_logged<L: LlmGenerator + ?Sized>(
    llm: &L,
    model: &str,
    prompt: &RenderedPrompt,
) -> Result<String, LlmError> {
    tracing::debug!(
        model,
        system = %prompt.system,
        user = %prompt.user,
        "[{}] LLM request",
        prompt.section
    );
    let reply = llm.generate(model, &prompt.system, &prompt.user);
    match &reply {
        Ok(text) if text.trim().is_empty() => {
            tracing::warn!("[{}] LLM returned an empty reply", prompt.section);
        }
        Ok(text) => tracing::debug!(
            bytes = text.len(),
            reply = %text,
            "[{}] LLM response",
            prompt.section
        ),
        Err(error) => tracing::debug!(
            error = %error.0,
            "[{}] LLM call failed",
            prompt.section
        ),
    }
    reply
}
