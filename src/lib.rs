//! LazyGUI feature owners and typed Git/configuration capabilities.
pub mod commit;
#[path = "views/commit_controls.rs"]
pub mod commit_controls;
pub mod diff;
#[path = "connectors/git/mod.rs"]
pub mod git;
#[cfg(test)]
#[path = "test_support/git.rs"]
mod git_fixture;
pub mod history;
pub mod input;
#[path = "storage/lazygit.rs"]
pub mod lazygit_config;
#[path = "views/process_probe.rs"]
pub mod process_probe;
#[path = "connectors/prompts/mod.rs"]
pub mod prompts;
pub mod repository;
pub mod working_tree;
