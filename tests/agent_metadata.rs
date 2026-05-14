//! End-to-end integration tests for agent metadata refs against real git.
//!
//! Exercises RFC-041 § metadata refs / STORY-124 AC1, AC3, AC8 via real
//! `git` invocations in a temp repo — no `GitRefOps` fakes.

mod common;

use anyhow::Result;
use chrono::DateTime;
use common::TestFixture;
use lazyspec::engine::agent_metadata::{
    AgentMetadata, AgentMetadataWriter, AgentStatus, GitRefAgentMetadata,
};
use lazyspec::engine::git_ref::GitCli;
use lazyspec::engine::read_agent_metadata;
use std::path::Path;
use std::process::Command;

fn sample_metadata(session_id: &str, status: AgentStatus) -> AgentMetadata {
    AgentMetadata {
        agent_id: "agent-x".to_string(),
        session_id: session_id.to_string(),
        doc_id: "STORY-124".to_string(),
        doc_type: "story".to_string(),
        status,
        started_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        last_event_at: DateTime::from_timestamp(1_700_000_500, 0).unwrap(),
        tokens_in: 10,
        tokens_out: 20,
        turn_count: 3,
        error: None,
        session_start_iteration_ids: vec!["ITER-179".to_string()],
    }
}

fn rev_list(root: &Path, refname: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["rev-list", refname])
        .current_dir(root)
        .output()?;
    assert!(
        output.status.success(),
        "git rev-list {} failed: {}",
        refname,
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect())
}

#[test]
fn write_twice_produces_two_commit_chain() -> Result<()> {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let writer = GitRefAgentMetadata::new(fixture.root().to_path_buf(), GitCli);

    let mut md = sample_metadata("sess-1", AgentStatus::Running);
    let sha1 = writer.write(&md)?;

    md.tokens_in = 100;
    md.last_event_at = DateTime::from_timestamp(1_700_001_000, 0).unwrap();
    let sha2 = writer.write(&md)?;

    assert_ne!(sha1, sha2);

    let commits = rev_list(fixture.root(), "refs/lazyspec/agents/sess-1")?;
    assert_eq!(commits.len(), 2, "expected 2-commit chain, got {:?}", commits);
    assert_eq!(commits[0], sha2, "head should be latest write");
    assert_eq!(commits[1], sha1, "parent should be first write");
    Ok(())
}

#[test]
fn mark_crashed_appends_chain_and_preserves_agent_id() -> Result<()> {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let writer = GitRefAgentMetadata::new(fixture.root().to_path_buf(), GitCli);

    let md = sample_metadata("sess-2", AgentStatus::Running);
    writer.write(&md)?;

    writer.mark_crashed("sess-2")?;

    let latest = read_agent_metadata(&GitCli, fixture.root(), "sess-2")?
        .expect("metadata should exist for sess-2");
    assert_eq!(latest.status, AgentStatus::Crashed);
    assert_eq!(latest.agent_id, "agent-x");
    assert_eq!(latest.doc_id, "STORY-124");
    assert_eq!(latest.tokens_in, 10);

    let commits = rev_list(fixture.root(), "refs/lazyspec/agents/sess-2")?;
    assert_eq!(
        commits.len(),
        2,
        "mark_crashed should preserve prior commit"
    );
    Ok(())
}

#[test]
fn read_agent_metadata_works_without_daemon() -> Result<()> {
    let (fixture, _bare) = TestFixture::with_git_remote();

    {
        let writer = GitRefAgentMetadata::new(fixture.root().to_path_buf(), GitCli);
        let md = sample_metadata("sess-3", AgentStatus::Running);
        writer.write(&md)?;
    }

    // Fresh GitCli, no writer, no daemon — just reading via the free fn.
    let got = read_agent_metadata(&GitCli, fixture.root(), "sess-3")?
        .expect("expected Some metadata for sess-3");
    assert_eq!(got, sample_metadata("sess-3", AgentStatus::Running));
    Ok(())
}

#[test]
fn read_agent_metadata_returns_none_for_unknown_session() -> Result<()> {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let got = read_agent_metadata(&GitCli, fixture.root(), "no-such-sid")?;
    assert!(got.is_none());
    Ok(())
}
