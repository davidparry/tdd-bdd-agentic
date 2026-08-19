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
