//! Workspace layout shared by every composition root (`main.rs`,
//! [`crate::mcp`], and [`crate::greenfield`]), so the spec location and
//! the kata layout are defined exactly once.

use crate::application::spec_service::ProjectLayout;

/// Where the requirements spec lives, relative to the project root.
pub const SPEC_PATH: &str = "requirements/requirements.json";

/// The workshop kata layout the frozen `get_requirement` tool reports,
/// byte-identical to the Java server.
pub fn workshop_layout() -> ProjectLayout {
    ProjectLayout {
        step_definitions:
            "kata/src/test/java/com/davidparry/workshop/kata/StringCalculatorSteps.java".into(),
        test_location: "kata/src/test/java/com/davidparry/workshop/kata/StringCalculatorTest.java"
            .into(),
        production_location:
            "kata/src/main/java/com/davidparry/workshop/kata/StringCalculator.java".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workshop_layout_names_the_kata_files() {
        let layout = workshop_layout();
        assert!(
            layout
                .step_definitions
                .ends_with("StringCalculatorSteps.java")
        );
        assert!(layout.test_location.ends_with("StringCalculatorTest.java"));
        assert!(
            layout
                .production_location
                .ends_with("StringCalculator.java")
        );
    }
}
