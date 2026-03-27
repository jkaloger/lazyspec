---
title: STORY-097 GitHub Issues config init and setup
type: iteration
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: STORY-097
---


## Goal

Add `[github]` config section parsing, extend `lazyspec init` to create labels for github-issues types, extend `lazyspec setup` to validate auth and run initial fetch, and add `.gitignore` entries for cache and issue map. Repo inference from `git remote get-url origin` when `repo` is omitted.

## Tasks

### T1: GithubConfig struct and parsing

Add a `GithubConfig` struct to `src/engine/config.rs` with `repo: Option<String>` and `cache_ttl: Option<String>` (default `"60s"`). Add an optional `github: Option<GithubConfig>` field to `RawConfig` and propagate it to `Config`. Parse the `[github]` section in `Config::parse`.

Files: `src/engine/config.rs`

### T2: Store field on TypeDef

Add `store: Option<String>` to `TypeDef` with serde default of `None` (meaning filesystem). This lets types declare `store = "github-issues"`. No behavioral dispatch yet, just config parsing.

Files: `src/engine/config.rs`

### T3: Repo inference from git remote

Add a helper function `infer_github_repo(project_root: &Path) -> Result<String>` that shells out to `git remote get-url origin` and parses `owner/repo` from SSH or HTTPS URLs. Used as fallback when `github.repo` is `None`.

Files: new `src/engine/github.rs` (module), `src/engine/mod.rs`

### T4: Init label creation for github-issues types

Extend `src/cli/init.rs` to detect types with `store = "github-issues"`. For each, shell out to `gh label create "lazyspec:{type}" --force` to create the type label. Resolve `repo` from config or inference. Skip label creation if `gh` is not installed (warn instead).

Files: `src/cli/init.rs`

### T5: Init gitignore entries

Extend `src/cli/init.rs` to append `.lazyspec/cache/` and `.lazyspec/issue-map.json` to `.gitignore` when any type uses `store = "github-issues"`. Only append lines that are not already present.

Files: `src/cli/init.rs`

### T6: Setup auth validation and initial fetch

Add or extend `src/cli/setup.rs` (new file if needed). For repos with github-issues types, run `gh auth status` and report pass/fail. On success, run `gh issue list --repo {repo} --label "lazyspec:{type}" --json number,title,body,labels,state,updatedAt` for each github-issues type and write results to `.lazyspec/cache/{type}/` and `.lazyspec/issue-map.json`.

Files: `src/cli/setup.rs` (new), `src/cli/mod.rs`, `src/main.rs` (register subcommand)

### T7: Validate warns on missing gh or auth

Extend `src/cli/validate.rs` to check for `gh` binary in `$PATH` and run `gh auth status`. Emit a warning if either check fails. Only run these checks when at least one type uses `store = "github-issues"`.

Files: `src/cli/validate.rs`

### T8: Wire up CLI subcommand for setup

Register `setup` as a CLI subcommand in `main.rs` / clap app. Route to `src/cli/setup.rs::run`.

Files: `src/main.rs`, `src/cli/mod.rs`

## Test Plan

### Unit tests

- `Config::parse` with `[github]` section present: assert `repo` and `cache_ttl` parsed
- `Config::parse` with `[github]` section absent: assert `github` is `None`
- `Config::parse` with `store = "github-issues"` on a type: assert `TypeDef.store` is set
- `cache_ttl` defaults to `"60s"` when omitted from `[github]`
- Repo inference parses `git@github.com:owner/repo.git` to `owner/repo`
- Repo inference parses `https://github.com/owner/repo.git` to `owner/repo`
- Repo inference parses `https://github.com/owner/repo` (no `.git` suffix)
- Gitignore append is idempotent (running twice does not duplicate entries)

### Integration tests (CLI)

- `lazyspec init` in a repo with a github-issues type: assert `.gitignore` contains cache and issue-map entries
- `lazyspec validate` with no `gh` installed: assert warning in output
- `lazyspec setup` with valid auth: assert cache directory created and issue-map written
- `lazyspec setup` with invalid auth: assert error message about authentication

### Manual verification

- Run `lazyspec init` in a test repo, confirm labels visible on GitHub
- Run `lazyspec setup` in a fresh clone, confirm cache populated
- Confirm `.gitignore` entries work (cache files not tracked)

## Notes

The `gh` CLI is the sole integration point for GitHub API operations. No direct HTTP calls. This keeps the implementation simple and delegates auth entirely to `gh auth`.

`cache_ttl` parsing (e.g. "60s" to a duration) can use a simple suffix parser or the `humantime` crate. Decide during implementation based on what's already in `Cargo.toml`.
