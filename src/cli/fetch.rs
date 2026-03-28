use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::GhIssueReader;
use crate::engine::github::resolve_repo;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use anyhow::{bail, Context, Result};
use std::path::Path;

pub fn run(root: &Path, config: &Config, gh: &dyn GhIssueReader, type_filter: Option<&str>, json: bool) -> Result<()> {
    let repo = resolve_repo(config, root)
        .context("Could not determine GitHub repo. Set [documents.github].repo in .lazyspec.toml")?;
    let mut issue_map = IssueMap::load(root)?;
    let cache = IssueCache::new(root);

    let gh_types: Vec<&str> = config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::GithubIssues)
        .map(|t| t.name.as_str())
        .collect();

    if gh_types.is_empty() {
        if json {
            println!("{{\"error\":\"no github-issues types configured\"}}");
        } else {
            println!("No github-issues types configured.");
        }
        return Ok(());
    }

    let types_to_fetch: Vec<&str> = if let Some(filter) = type_filter {
        if !gh_types.contains(&filter) {
            bail!("type '{}' is not a github-issues type", filter);
        }
        vec![filter]
    } else {
        gh_types
    };

    let mut summaries = Vec::new();

    for type_name in &types_to_fetch {
        let type_def = config
            .type_by_name(type_name)
            .ok_or_else(|| anyhow::anyhow!("type '{}' not found in config", type_name))?;

        let all_type_names: Vec<String> = config.documents.types.iter().map(|t| t.name.clone()).collect();
        let result = cache.fetch_all(root, type_def, gh, &repo, &mut issue_map, &all_type_names)?;

        summaries.push(TypeSummary {
            type_name: type_name.to_string(),
            fetched: result.fetched,
            new: result.new,
            removed: result.removed,
        });
    }

    issue_map.save(root)?;

    if json {
        let json_out: Vec<serde_json::Value> = summaries
            .iter()
            .map(|s| {
                serde_json::json!({
                    "type": s.type_name,
                    "fetched": s.fetched,
                    "new": s.new,
                    "removed": s.removed,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_out)?);
    } else {
        for s in &summaries {
            println!(
                "{}: fetched {}, {} new, {} removed",
                s.type_name, s.fetched, s.new, s.removed
            );
        }
    }

    Ok(())
}

struct TypeSummary {
    type_name: String,
    fetched: usize,
    new: usize,
    removed: usize,
}

