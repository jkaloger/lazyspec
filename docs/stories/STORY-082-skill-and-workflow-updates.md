---
title: Skill and Workflow Updates
type: story
status: draft
author: jkaloger
date: 2026-03-24
tags: []
related:
- implements: docs/rfcs/RFC-031-spec-certification-and-drift-detection.md
---


## Context

Certification (RFC-031) introduces specs as the contract layer between design intent and implementation. The existing skill set was built around the RFC -> Story -> Iteration pipeline. With specs as a first-class document type that own AC and `@ref` directives, the agent skills need to recognise specs as entry points, consume drift/certification CLI commands, and guide the updated workflow where iterations implement specs directly.

This story covers the skill file changes only. It assumes the `spec` document type (Story 1), blob pinning (Story 2), drift detection engine (Story 3), certification CLI (Story 4), and `affects` relationship (Story 5) are already implemented.

## Acceptance Criteria

### AC: write-spec-skill-created

- **Given** the `.claude/skills/` directory
  **When** the `/write-spec` skill is invoked
  **Then** a `write-spec/SKILL.md` file exists with instructions that guide through: identifying scope, writing `index.md` with `@ref` directives and prose, writing `story.md` with given/when/then AC only (no `@ref`), and validating that all refs resolve

### AC: write-spec-encourages-tight-scope

- **Given** the `/write-spec` skill definition
  **When** an agent reads the skill
  **Then** the skill contains guidance that specs should be scoped tightly enough to certify, and that `@ref` is preferred over prose for describing code structure

### AC: certify-spec-skill-created

- **Given** the `.claude/skills/` directory
  **When** the `/certify-spec` skill is invoked
  **Then** a `certify-spec/SKILL.md` file exists with a workflow that: runs `lazyspec drift`, presents signals, runs `lazyspec certify` on confirmation, blocks on test failure, presents frontmatter diff, and runs `lazyspec validate` afterward

### AC: certify-spec-blocks-on-failure

- **Given** the `/certify-spec` skill definition
  **When** an agent reads the certification workflow steps
  **Then** the skill explicitly gates certification on test passage and presents failures to the user before allowing retry

### AC: audit-cert-skill-created

- **Given** the `.claude/skills/` directory
  **When** the `/audit-cert` skill is invoked
  **Then** an `audit-cert/SKILL.md` file exists that: runs `lazyspec status` to find specs with signals, presents them for selection, assesses conformance gaps vs intentional changes, documents findings as an audit, and proposes next steps without auto-creating iterations

### AC: plan-work-recognises-specs

- **Given** the existing `/plan-work` skill
  **When** a user describes work in a domain covered by a spec
  **Then** the updated skill checks `lazyspec drift` output for stale specs, classifies "spec conformance work" as a work type, and routes to iterations that implement the spec directly

### AC: plan-work-surfaces-stale-specs

- **Given** the updated `/plan-work` skill definition
  **When** an agent reads the preflight and classification steps
  **Then** the skill instructs agents to run `lazyspec drift` during preflight and surface stale specs even when the user did not mention certification

### AC: create-iteration-accepts-spec-parent

- **Given** the existing `/create-iteration` skill
  **When** the skill is updated for spec support
  **Then** the skill accepts a spec (not just a story) as a parent, reads AC from the spec's `story.md`, and links the iteration via `implements`

### AC: build-checks-affected-specs

- **Given** the existing `/build` skill
  **When** a build completes
  **Then** the updated skill checks which specs are affected by the changes, surfaces specs with new signals, and suggests running `/certify-spec` as a prompt (not automatic)

### AC: review-iteration-verifies-spec-conformance

- **Given** the existing `/review-iteration` skill
  **When** the iteration `implements` a spec
  **Then** the updated skill checks that `@ref` targets still resolve and flags drift in affected specs (without blocking the review, since drift during active development is expected)

### AC: create-audit-spec-conformance-type

- **Given** the existing `/create-audit` skill
  **When** the skill is updated for certification
  **Then** the skill supports a "spec conformance" audit type, derives criteria from `@ref` targets and prose claims, and runs `lazyspec drift` as preflight to seed the audit with known signals

### AC: workflow-diagrams-updated

- **Given** all modified skills (plan-work, create-iteration, build, review-iteration, create-audit)
  **When** an agent reads the workflow position diagrams
  **Then** the d2 diagrams reflect the updated pipeline where specs are an entry point and iterations can implement specs directly

### AC: skill-forbidden-actions-consistent

- **Given** all new and modified skills
  **When** an agent reads the Forbidden Actions section
  **Then** each skill includes the standard lazyspec forbidden-actions block (no direct file writes, no editing unread documents) and any skill-specific prohibitions

## Scope

### In Scope

- Creating three new skill files: `write-spec/SKILL.md`, `certify-spec/SKILL.md`, `audit-cert/SKILL.md`
- Modifying five existing skill files: `plan-work/SKILL.md`, `create-iteration/SKILL.md`, `build/SKILL.md`, `review-iteration/SKILL.md`, `create-audit/SKILL.md`
- Updating workflow d2 diagrams in modified skills to reflect spec-aware pipeline
- Adding spec-related steps and classification to existing workflows

### Out of Scope

- The `spec` document type engine implementation (Story 1)
- Blob pinning implementation (Story 2)
- Drift detection engine implementation (Story 3)
- Certification CLI command implementation (Story 4)
- `affects` relationship engine implementation (Story 5)
- Changes to `resolve-context`, `create-story`, or `write-rfc` skills (these may be deprecated later but are not modified here)
- Runtime behaviour of the skills (this story produces skill definition files, not executable code)
