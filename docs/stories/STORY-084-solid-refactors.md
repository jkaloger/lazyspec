---
title: SOLID Refactors
type: story
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/rfcs/RFC-032-code-quality-remediation-audit-007-findings.md
---



## Context

RFC-032 stream 6. Lower-priority structural improvements for long-term maintainability. Should follow Story 5 (engine module splits) so trait introductions target the new module structure. The filesystem abstraction (6d) has the widest blast radius and may not pay for itself if the rate of new validation rules remains low.

## Acceptance Criteria

### AC1: Validation Rules Trait (Stream 6a)

- Given the ValidationIssue enum requiring modification of enum and all match arms to add a rule
  When a ValidationRule trait is introduced with check() and severity() methods
  Then each rule is a struct implementing the trait, and validate_full() orchestrates via Vec<Box<dyn ValidationRule>>

- Given the trait-based validation system
  When a new validation rule is needed
  Then it can be added by implementing the trait without modifying existing code

### AC2: Relation Type Round-trip (Stream 6b)

- Given RelationType as a four-variant enum with string matching scattered across link.rs
  When FromStr and Display are implemented
  Then string mapping is centralised in the trait impls and removed from match blocks in link.rs

### AC3: Config Decomposition (Stream 6c)

- Given the Config struct with fields for types, rules, directories, templates, naming, tui, sqids, and reserved ranges
  When fields are grouped into DocumentConfig, FilesystemConfig, UiConfig, and RulesConfig sub-structs
  Then all config.field accesses are updated to config.documents.field (etc.) and Config::load remains the single construction point

### AC4: Filesystem Abstraction (Stream 6d)

- Given direct std::fs calls throughout the codebase preventing isolated unit testing
  When a FileSystem trait is introduced with read_to_string, write, rename, and read_dir
  Then production code uses RealFileSystem and tests can inject MockFileSystem or InMemoryFileSystem

- Given the FileSystem trait threaded through function signatures
  When engine and CLI tests run
  Then at least one test demonstrates mock filesystem injection

## Scope

### In Scope

- ValidationRule trait and migration of existing rules
- FromStr/Display for RelationType
- Config sub-struct decomposition
- FileSystem trait with RealFileSystem production impl

### Out of Scope

- New validation rules (just migrate existing ones)
- New relation types
- Data-driven relation types (rejected in RFC)
- Comprehensive mock test coverage (one demonstration test suffices)
