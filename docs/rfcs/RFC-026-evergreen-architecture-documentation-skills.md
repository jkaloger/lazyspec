---
title: "Evergreen Architecture Documentation Skills"
type: rfc
status: draft
author: "jkaloger"
date: 2026-03-15
tags:
  - skills
  - architecture
  - documentation
  - ai
related:
  - related to: docs/rfcs/RFC-002-ai-driven-workflow.md
  - related to: docs/rfcs/RFC-019-inline-type-references-with-ref.md
---

## Problem

Architecture documentation goes stale. ARCH-001 through ARCH-005 were written as a snapshot of the codebase at a point in time. As code evolves, the docs drift: new modules appear without corresponding sections, `@ref` directives point at renamed or restructured symbols, diagrams depict outdated control flow, and descriptions of behavior diverge from implementation.

AUDIT-001 already found this drift after the initial architecture documentation commit. The audit was manual and one-off. There's no mechanism to detect drift continuously or to surface it to developers when they make changes that affect documented architecture.

The `@ref` directive system helps (broken refs are detectable), but refs only cover specific code symbols. Higher-level descriptions of how modules interact, what data flows where, and why decisions were made are prose, and prose doesn't break loudly.

## Intent

Create Claude Code skills that keep architecture documentation in sync with the codebase. The skills operate at two levels:

1. **Reactive**: After significant code changes, diff the architecture docs against the actual module structure and flag stale sections.
2. **Generative**: When a new module or significant feature lands, generate draft architecture documentation or propose updates to existing docs.

These skills are designed for lazyspec's own spec documents but should be general enough for any project using lazyspec-style spec documents.

## Design

### Skill: `/audit-arch`

A skill that runs a staleness audit of architecture documentation. It compares the documented state against the current codebase and produces findings.

**What it checks:**

| Check | How | Severity |
|-------|-----|----------|
| Broken `@ref` directives | Run `lazyspec validate`, filter for ref errors | error |
| Missing modules | Compare `src/` module tree against ARCH doc sections | warning |
| Stale struct/enum descriptions | Compare `@ref` targets' current signatures against prose descriptions | warning |
| Diagram accuracy | Compare d2 diagram node names against actual module/struct names | info |
| Orphaned sections | ARCH sections describing modules that no longer exist | warning |

**Output:** A lazyspec audit document (using the existing `audit` type) with findings, linked to the relevant ARCH documents. This integrates with the existing audit workflow from `create-audit`.

**Invocation:**

```
/audit-arch                    # audit all ARCH docs
/audit-arch ARCH-005           # audit a specific document
/audit-arch --since HEAD~5     # only check modules changed in last 5 commits
```

The `--since` flag uses `git diff` to scope the audit to recently changed files, reducing noise and execution time for incremental checks.

### Skill: `/update-arch`

A skill that proposes updates to architecture documentation based on code changes.

**Workflow:**

1. Identify what changed (via `git diff` against a base ref, or by examining a specific module)
2. Read the relevant ARCH document(s)
3. Propose edits: new sections for new modules, updated descriptions for changed behavior, corrected diagrams
4. Present the diff to the user for approval before writing

**Invocation:**

```
/update-arch                   # propose updates for all stale sections
/update-arch ARCH-003          # propose updates for a specific doc
/update-arch src/engine/       # propose updates based on changes in a directory
```

The skill uses the `@ref` system to anchor its understanding. When a ref target changes, the skill can read the new code, compare it to the prose around the ref, and propose updated descriptions.

> [!NOTE]
> This skill proposes, it doesn't auto-commit. Architecture docs are design records and should be reviewed by a human. The skill reduces the effort of maintaining them, not the oversight.

### Skill: `/scaffold-arch`

A skill for bootstrapping architecture documentation for new modules or subsystems.

**Workflow:**

1. Scan the target directory for public types, traits, and functions
2. Identify relationships (imports, trait implementations, module hierarchy)
3. Generate a draft ARCH document with: overview, key types, data flow diagram (d2), relationships to existing ARCH docs
4. Create the document via `lazyspec create arch` and populate it

**Invocation:**

```
/scaffold-arch src/tui/        # generate ARCH doc for the TUI module
/scaffold-arch src/engine/ref_expansion.rs  # generate for a specific file
```

### Integration with Existing Infrastructure

These skills build on:

- **`@ref` directives** (RFC-019): The primary mechanism for anchoring docs to code. Skills check ref validity and use ref targets to understand what code a section describes.
- **`lazyspec validate`**: Already checks for broken refs. The audit skill extends this with higher-level checks.
- **`create-audit` skill**: The audit output uses the existing audit document type and workflow.
- **`d2` diagrams**: Skills generate and update d2 diagrams in architecture docs, using the existing diagram rendering pipeline.

### Trigger Suggestions

While the skills are invoked manually, the README or CLAUDE.md could suggest running them:

- After merging a PR that touches `src/` significantly
- Before a release, as part of the release checklist
- When `lazyspec validate` reports ref warnings in ARCH docs
- As part of the `/review-iteration` skill chain (if the iteration touched architecture-relevant code)

Automation via hooks (e.g. post-merge or pre-release) is a future consideration, not part of this RFC.

## Stories

1. **`/audit-arch` skill** -- Staleness detection across ARCH docs. Module tree comparison, ref validation, diagram node checking. Outputs an audit document with findings. Supports `--since` for incremental audits.

2. **`/update-arch` skill** -- Proposes edits to existing ARCH docs based on code changes. Reads refs, compares prose to current code, generates diffs. User approval before writing.

3. **`/scaffold-arch` skill** -- Generates draft ARCH docs for new modules. Type scanning, relationship detection, d2 diagram generation. Creates documents via `lazyspec create`.
