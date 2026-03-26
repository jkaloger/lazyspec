---
title: Metrics Mode
type: story
status: draft
author: jkaloger
date: 2026-03-05
tags:
- tui
related:
- implements: RFC-006
---


## Context

Teams using lazyspec need visibility into how documents flow through the system over time. Are drafts getting stuck? Is the team producing RFCs at a steady pace? Currently this requires manually counting documents or running CLI queries. Metrics mode provides at-a-glance project health through sparkline charts and summary statistics.

## Acceptance Criteria

### AC1: Per-type sparklines

- **Given** the TUI is in Metrics mode
  **When** the left panel renders
  **Then** it shows a sparkline for each document type representing document creation volume over time

### AC2: Sparkline time granularity

- **Given** documents exist with dates spanning multiple weeks
  **When** sparklines render
  **Then** each data point represents one week of document creation, based on the `date` frontmatter field

### AC3: Status flow chart

- **Given** the TUI is in Metrics mode
  **When** the right panel renders
  **Then** it shows sparklines for each status (draft, review, accepted, rejected, superseded) with status-appropriate colours

### AC4: Summary statistics

- **Given** the TUI is in Metrics mode
  **When** the right panel renders
  **Then** it shows total document count, documents created this week, and the age of the oldest draft

### AC5: Validation summary

- **Given** validation errors exist in the store
  **When** Metrics mode renders
  **Then** the summary statistics panel shows validation error counts (e.g. "2 broken links, 1 unlinked iteration")

### AC6: Metrics update on store changes

- **Given** the TUI is in Metrics mode
  **When** a document is created or deleted
  **Then** the sparklines and statistics reflect the change

## Scope

### In Scope

- Weekly count aggregation from document `date` fields
- Sparkline rendering using ratatui's `Sparkline` widget
- Per-type sparklines (left panel)
- Per-status sparklines (right panel)
- Summary statistics (total, this week, oldest draft)
- Validation error counts in summary

### Out of Scope

- Interactive drill-down from metrics to filtered document lists
- Custom time ranges or date pickers
- Historical status tracking (we can only see current status, not when it changed)
