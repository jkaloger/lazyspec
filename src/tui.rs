#[cfg(feature = "agent")]
pub mod agent;
pub mod content;
pub mod infra;
pub mod state;
pub mod views;

pub use infra::event_loop::run;

use crate::engine::config::{Config, StoreBackend};

/// Whether the project has any type the background poll refreshes: github
/// issues/milestones or clickup tasks. Gates the poll loop, the header sync
/// face + countdown, and the `last_sync` seed so all three agree -- a
/// milestone-only or clickup-only project polls and shows the tick just like a
/// github-issues project.
pub(crate) fn has_pollable_types(config: &Config) -> bool {
    config.documents.types.iter().any(|t| {
        t.store == StoreBackend::GithubIssues
            || t.store == StoreBackend::GithubMilestones
            || t.store == StoreBackend::ClickupTasks
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::TypeDef;

    // Gate: a project whose only GitHub-backed type is github-milestones must
    // still poll, so a milestone created after launch appears live in the list.
    #[test]
    fn milestone_only_project_is_pollable() {
        let mut config = Config::default();
        config.documents.types = vec![TypeDef::test_fixture(
            "milestone",
            StoreBackend::GithubMilestones,
        )];

        assert!(has_pollable_types(&config));
    }

    // Gate: a clickup-only project must poll too, so tasks created after launch
    // appear live without a manual fetch — same parity as github types.
    #[test]
    fn clickup_only_project_is_pollable() {
        let mut config = Config::default();
        config.documents.types = vec![TypeDef::test_fixture("task", StoreBackend::ClickupTasks)];

        assert!(has_pollable_types(&config));
    }

    // Gate: a project with no pollable types must not poll.
    #[test]
    fn project_without_gh_types_is_not_pollable() {
        let mut config = Config::default();
        config.documents.types = vec![
            TypeDef::test_fixture("doc", StoreBackend::Filesystem),
            TypeDef::test_fixture("note", StoreBackend::Filesystem),
        ];

        assert!(!has_pollable_types(&config));
    }
}
