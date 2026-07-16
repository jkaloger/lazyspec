# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Removed

- **Breaking:** remove the lease subsystem: `claim`, `release`, `leases`, and `heartbeat` subcommands are gone and now fail with the standard unknown-subcommand error
- the `[coordination]` config block is no longer read; configs still carrying it parse fine and the block is ignored
- migration: leftover `refs/lazyspec/leases/*` refs are orphaned and harmless; prune with `git for-each-ref --format='%(refname)' refs/lazyspec/leases | while read -r ref; do git update-ref -d "$ref"; done`

### Changed

- git-ref document stores are now local-write only: writes land in the local ref and remote sync happens via `lazyspec fetch`; there is no automatic remote push

## [0.9.2](https://github.com/jkaloger/lazyspec/compare/v0.9.1...v0.9.2) - 2026-07-11

### Other

- consolidate release.yml into publish.yml release-plz upload ([#78](https://github.com/jkaloger/lazyspec/pull/78))
- priority queue idea

## [0.9.1](https://github.com/jkaloger/lazyspec/compare/v0.9.0...v0.9.1) - 2026-07-10

### Added

- emit config JSON Schema via config schema command ([#76](https://github.com/jkaloger/lazyspec/pull/76))
- scope clickup-tasks types to a custom task type ([#75](https://github.com/jkaloger/lazyspec/pull/75))
- add ClickUp document store ([#74](https://github.com/jkaloger/lazyspec/pull/74))
- classify github-issues types by native issue type and custom tag ([#73](https://github.com/jkaloger/lazyspec/pull/73))
- *(release)* add tag-driven release + crates.io publish pipeline ([#71](https://github.com/jkaloger/lazyspec/pull/71))
- add read-only web view and native macOS desktop app ([#70](https://github.com/jkaloger/lazyspec/pull/70))
- package skills and convention hook as a Claude Code plugin ([#69](https://github.com/jkaloger/lazyspec/pull/69))
- *(gh)* better GitHub integration ([#67](https://github.com/jkaloger/lazyspec/pull/67))

### Fixed

- *(milestones)* restrict milestone link source to github-issues docs, fix TUI milestone display ([#68](https://github.com/jkaloger/lazyspec/pull/68))

### Other

- update doc statuses
- update game dev loop with iter
- example game dev loop
- add CODEOWNERS to require human review on main
