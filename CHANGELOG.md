# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.1](https://github.com/jkaloger/lazyspec/compare/v0.11.0...v0.11.1) - 2026-08-13

### Added

- *(skills)* prevent `advance` command hallucination
- board-driven lifecycle via status_authority ([#95](https://github.com/jkaloger/lazyspec/pull/95))
- *(graph)* bidirectional graph view

### Fixed

- re-check remote body before unlocked issue_edit
- slow graph
- *(clickup)* cache regression
- *(clickup)* attr doesn't update custom fields

### Other

- compare body not updated_at in the pre-write re-check
- Batch Github Reads ([#96](https://github.com/jkaloger/lazyspec/pull/96))
- disable flakey test
- lean execute ([#92](https://github.com/jkaloger/lazyspec/pull/92))

## [0.11.0](https://github.com/jkaloger/lazyspec/compare/v0.10.0...v0.11.0) - 2026-07-27

### Added

- *(github)* relationship-driven native sub-issues, ordinary relations on GitHub-authored issues ([#90](https://github.com/jkaloger/lazyspec/pull/90))
- *(github)* read native blocks/blocked-by back on fetch
- *(github)* native blocks/blocked-by write path via issue dependencies
- [**breaking**] ranked fuzzy search across engine, TUI, and CLI ([#86](https://github.com/jkaloger/lazyspec/pull/86))
- interactive config wizard, spinners, and mascot ([#84](https://github.com/jkaloger/lazyspec/pull/84))

### Fixed

- *(github)* honour github_issue_tag as create label without issue type
- table truncation causing panic

### Other

- update iter status
- more readme updates
- readme updates
- doc updates

## [0.10.0](https://github.com/jkaloger/lazyspec/compare/v0.9.2...v0.10.0) - 2026-07-18

### Added

- *(engine)* github types inherit canonical open/closed lifecycle
- *(assignee)* display assignee across TUI, web, and CLI
- *(assignee)* inherit and write through remote assignee for github and clickup
- *(assignee)* first-class assignee field on DocMeta with update flag and JSON
- cross-clone-safe git-ref number allocation
- push git-ref mutations to configured remote
- [git-ref] remote config as single source of truth
- TUI o keybind opens doc in browser or viewer
- engine open-target resolution and show --open
- list custom attributes in preview header
- configurable doc-table columns via [tui.table]
- config-driven status colours with stable unknown-status fallback
- config-write serializes attribute defs, add-type --attribute flag
- add --json to delete, link, unlink, ignore, unignore
- [**breaking**] remove lease subsystem — claim/release/leases/heartbeat, gates, coordination config
- tag add/remove commands with backend sync ([#79](https://github.com/jkaloger/lazyspec/pull/79))

### Fixed

- *(engine)* skip lazyspec label on github docs with a native issue type
- *(engine)* close remote issue/milestone on terminal-state transition
- *(engine)* inherit remote issue/milestone open-closed into lifecycle on sync
- *(engine)* seed github-backed docs with first lifecycle state at birth
- *(engine,cli)* surface git-ref push outcome in mutation JSON
- *(cli)* show --open whitespace-splits viewer command
- *(cli)* show --open --json emits ambiguous_id JSON error
- *(tui)* wrap-mode row height only measures configured tag/provenance columns
- create seeds first lifecycle state, fix repairs invalid status
- non-blocking UI-thread store lock, editor args, sync timeouts
- mutation correctness — link null-related panic, git-ref update key drop and quoting, shorthand ambiguity, create --json id
- crash-safe sidecar cache persistence
- remove duplicate attributes key from bug type config

### Other

- derive Default for PushOutcome to satisfy clippy
- story updates
- story updates
- advance STORY-222 to in-progress (iterations 320-322 complete, pending sign-off)
- advance STORY-223 to in-progress (iterations 318-319 complete, pending sign-off)
- advance ITERATION-316 to complete
- bug report
- iterations for bug-bash fixes and assignee feature (ITERATION-313..322, STORY-223)
- card bug-bash findings BUG-003..008 and assignee story STORY-222
- remove release asset uploads and tauri build from publish
- docs
- card bug-bash stories, bugs, git-ref liveness audit, iterations
- hoist doc ops into engine::ops layer, dedupe store wiring
- purge lease docs from README, sync command and keybind tables, changelog
- codebase health audit AUDIT-018 with stories and iterations
- bug type
- readme updates ([#81](https://github.com/jkaloger/lazyspec/pull/81))
- update deps
- dedupe STORY-204/ITERATION-289, add lease-removal RFC, trim RFC-060 scope
- comments rfc

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
