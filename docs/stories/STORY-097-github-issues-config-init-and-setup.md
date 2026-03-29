---
title: GitHub Issues config init and setup
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: RFC-037
---


## Context

The GitHub Issues document store requires initial configuration and setup. Users must configure the repo location and cache behavior in `.lazyspec.toml`, then initialize labels in their repo and set up authentication and initial data fetch for new clones.

## Acceptance Criteria

### AC: Config section parsing

- **Given** a `.lazyspec.toml` with a `[github]` config section
  **When** lazyspec loads the configuration
  **Then** the repo and cache_ttl fields are parsed and available

### AC: Repo inference from git remote

- **Given** a `.lazyspec.toml` without a repo field in `[github]`
  **When** lazyspec needs the repo address
  **Then** it infers the repo from `git remote get-url origin`

### AC: Init creates labels

- **Given** `lazyspec init` is run with a github-issues document type
  **When** the command executes
  **Then** it creates `lazyspec:{type}` labels on the repo via `gh label create`

### AC: Init updates gitignore

- **Given** `lazyspec init` is run with a github-issues document type
  **When** the command completes
  **Then** it adds `.lazyspec/cache/` and `.lazyspec/issue-map.json` to `.gitignore`

### AC: Setup validates auth and fetches

- **Given** `lazyspec setup` is run on a fresh clone
  **When** the command executes
  **Then** it validates `gh auth status` and runs an initial fetch

### AC: Validate warns on missing gh or auth

- **Given** `lazyspec validate` is run
  **When** `gh` is not installed or not authenticated
  **Then** it emits a warning about the missing or unauthenticated state

## Scope

### In Scope

- `[github]` config section with repo and cache_ttl fields
- Repo inference from git remote origin
- Label creation during init via gh CLI
- Gitignore entries for cache and issue map
- Auth validation and initial fetch in setup
- Validation warnings for missing gh or auth

### Out of Scope

- Native HTTP authentication modes
- GitHub App installation tokens
