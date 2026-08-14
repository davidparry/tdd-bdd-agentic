//! Pure business logic. Nothing in this module performs IO; anything the
//! logic needs from the outside world arrives through [`crate::ports`].

pub mod feature;
pub mod generation;
pub mod language;
pub mod model;
pub mod prompts;
pub mod proposal;
pub mod refiner;
pub mod scaffold;
pub mod spec_validator;
pub mod steps;
pub mod tdd;
pub mod workflow;
