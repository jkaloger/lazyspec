---
title: "Validation Pipeline"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Overview

The validation pipeline is a read-only pass over the document store that produces a categorized list of diagnostics. @ref src/engine/validation.rs#validate_full iterates a fixed set of checkers and partitions their output into errors and warnings via @ref src/engine/validation.rs#ValidationResult, which holds two separate `Vec<ValidationIssue>` fields. The pipeline never mutates the store.

Each diagnostic is a variant of @ref src/engine/validation.rs#ValidationIssue. Variants carry enough context (paths, IDs, slugs) for both human-readable display and structured JSON output. The `Display` impl on `ValidationIssue` formats each variant into a single-line message.

## Checker Trait

Every rule implements @ref src/engine/validation.rs#Checker, a trait with one method: `check(&self, store, config) -> Vec<(Severity, ValidationIssue)>`. Each checker returns issues paired with their severity. The `validate_full` function collects these pairs and routes them into the `errors` or `warnings` vectors based on the `Severity` enum (which has two variants: `Error` and `Warning`).

Documents with `validate_ignore: true` in their frontmatter are skipped by every checker. Each rule checks `meta.validate_ignore` at the top of its iteration loop and continues past ignored documents.

## Rules

### BrokenLinkRule

@ref src/engine/validation.rs#BrokenLinkRule iterates every document's `related` entries and checks whether the target path exists in the store. A missing target produces a `BrokenLink` error.

For targets that do exist and are connected via a hierarchy link (derived from config-defined `ParentChild` rules), the rule performs additional status checks. Linking to a `Rejected` parent is an error (`RejectedParent`). An `Accepted` child linking to a `Superseded` parent produces a `SupersededParent` warning. An `Accepted` child linking to a non-`Accepted` parent in a hierarchy relationship produces an `OrphanedAcceptance` warning.

### ParentLinkRule

@ref src/engine/validation.rs#ParentLinkRule enforces config-driven structural requirements. It evaluates two rule shapes from the config: `ParentChild` rules require that a document of the child type has at least one relation of the specified link type pointing to a document of the parent type. `RelationExistence` rules require that a document of a given type has at least one relation of any kind. Both rule shapes use the severity specified in the config, so the same rule can be an error or a warning depending on project configuration.

### StatusConsistencyRule

@ref src/engine/validation.rs#StatusConsistencyRule checks upward status consistency using the store's reverse link index. For each parent document in `Draft` or `Review` status, it collects all children connected via hierarchy links. If every child is `Accepted`, it emits an `AllChildrenAccepted` warning. If only some children are `Accepted` (and the parent is not), it emits an `UpwardOrphanedAcceptance` warning for each accepted child individually.

### DuplicateIdRule

@ref src/engine/validation.rs#DuplicateIdRule builds a map from extracted document IDs to paths. Any ID shared by two or more documents (excluding those with empty IDs or `validate_ignore`) produces a `DuplicateId` error. The paths in the diagnostic are sorted for deterministic output.

### AcSlugFormatRule

@ref src/engine/validation.rs#AcSlugFormatRule applies only to `story.md` files that are children of a spec document. It reads the file body and scans for lines starting with `### AC:`. Each slug after the prefix is validated against a kebab-case regex (`^[a-z0-9]+(-[a-z0-9]+)*$`). Empty slugs, duplicate slugs within the same file, and non-matching slugs each produce an `InvalidAcSlug` warning.

### RefScopeRule

@ref src/engine/validation.rs#RefScopeRule applies only to spec index documents (files named `index.md` with doc type `spec`, or spec documents with no parent). It parses `@ref` directives from the body and counts distinct ref paths. If the count exceeds `config.ref_count_ceiling` (default 15), it emits a `RefCountExceeded` warning. If the refs span more than 3 distinct module prefixes (the first two path segments), it emits a `CrossModuleRefs` warning.

### OrphanRefRule

@ref src/engine/validation.rs#OrphanRefRule also targets spec index documents. For each `@ref` directive in the body, it checks whether the referenced file path exists on disk relative to the project root. A missing target produces an `OrphanRef` warning.
