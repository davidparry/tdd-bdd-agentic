//! The prompt catalog: every LLM call's system and user prompt lives in
//! `prompts/prompts.toml` (embedded at compile time), written as MiniJinja
//! templates. The prompt builders render a section with their dynamic
//! context; the Rust source holds no prompt wording.

use std::sync::OnceLock;

use minijinja::Environment;
use serde::Serialize;

/// The embedded prompt catalog - the single source of prompt wording.
const PROMPTS_TOML: &str = include_str!("../../prompts/prompts.toml");

/// The sections the catalog must hold, one per LLM call.
pub const SECTIONS: [&str; 6] = [
    "proposal",
    "rewording",
    "polish",
    "implementation",
    "advice",
    "next_step",
];

/// One LLM call's prompts: the system prompt carries the model's role
/// and rules, the user prompt carries the call's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPrompt {
    pub system: String,
    pub user: String,
}

#[derive(serde::Deserialize)]
struct PromptPair {
    system: String,
    user: String,
}

/// The templates, registered as `<section>.system` / `<section>.user`.
/// The catalog is a compile-time asset: a malformed file or template is
/// a build defect, surfaced loudly on first use and covered by tests.
fn environment() -> &'static Environment<'static> {
    static ENV: OnceLock<Environment<'static>> = OnceLock::new();
    ENV.get_or_init(|| {
        let catalog: std::collections::BTreeMap<String, PromptPair> = toml::from_str(PROMPTS_TOML)
            .expect("prompts/prompts.toml is a compile-time asset and must parse");
        let mut env = Environment::new();
        for section in SECTIONS {
            let pair = catalog
                .get(section)
                .unwrap_or_else(|| panic!("prompts/prompts.toml is missing [{section}]"));
            for (role, source) in [("system", &pair.system), ("user", &pair.user)] {
                env.add_template_owned(format!("{section}.{role}"), source.clone())
                    .unwrap_or_else(|e| {
                        panic!("prompt template {section}.{role} does not compile - {e}")
                    });
            }
        }
        env
    })
}

/// Render one section's system and user templates with the same context.
pub(crate) fn render(section: &str, context: impl Serialize) -> RenderedPrompt {
    let value = minijinja::Value::from_serialize(&context);
    RenderedPrompt {
        system: render_one(&format!("{section}.system"), &value),
        user: render_one(&format!("{section}.user"), &value),
    }
}

fn render_one(name: &str, context: &minijinja::Value) -> String {
    environment()
        .get_template(name)
        .unwrap_or_else(|_| panic!("no prompt template named {name}"))
        .render(context)
        .unwrap_or_else(|e| panic!("prompt template {name} failed to render - {e}"))
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_registers_a_system_and_a_user_template() {
        let env = environment();
        for section in SECTIONS {
            for role in ["system", "user"] {
                assert!(
                    env.get_template(&format!("{section}.{role}")).is_ok(),
                    "missing template {section}.{role}"
                );
            }
        }
    }

    #[test]
    fn rendering_fills_the_context_into_both_prompts() {
        let prompt = render(
            "proposal",
            minijinja::context! { description => "sum numbers" },
        );
        assert!(prompt.system.contains("ONLY a JSON array"));
        assert!(
            prompt
                .user
                .contains("<description>\nsum numbers\n</description>")
        );
    }
}
