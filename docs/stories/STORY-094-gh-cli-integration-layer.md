---
title: gh CLI integration layer
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: RFC-037
---




## Context

The initial implementation delegates all GitHub API operations to the `gh` CLI rather than using a native HTTP client. This approach leverages the existing gh tooling which handles authentication, rate limiting, and API versioning. Operations include issue creation, editing, listing, viewing, closing, label management, and auth validation. All structured data is extracted from JSON output produced by gh commands.

## Acceptance Criteria

### AC: Execute gh issue create

- **Given** an authenticated gh environment
  **When** executing `gh issue create` with title, body, and labels
  **Then** the command returns JSON output containing the new issue ID and URL

### AC: Execute gh issue edit

- **Given** an existing issue in a repository
  **When** executing `gh issue edit` to update body and labels
  **Then** the command completes successfully and the issue is updated in GitHub

### AC: Execute gh issue list with filtering

- **Given** a repository with multiple issues
  **When** executing `gh issue list` with label filter and requesting JSON output
  **Then** the command returns JSON array of filtered issues

### AC: Execute gh issue view

- **Given** an existing issue
  **When** executing `gh issue view` with JSON output flag
  **Then** the command returns JSON containing full issue details

### AC: Execute gh issue close and reopen

- **Given** an existing issue
  **When** executing `gh issue close` or `gh issue reopen`
  **Then** the command completes successfully and the issue state changes in GitHub

### AC: Manage labels via gh label create

- **Given** a repository
  **When** executing `gh label create` with label name, description, and color
  **Then** the label is created in GitHub or error is returned if it already exists

### AC: Validate auth with gh auth status

- **Given** a system with gh CLI installed
  **When** executing `gh auth status`
  **Then** the output indicates whether a valid authentication token exists

### AC: Handle gh not installed

- **Given** a system without gh CLI
  **When** attempting to execute any gh command
  **Then** a clear error is returned indicating gh CLI is not installed

### AC: Handle authentication failure

- **Given** an invalid or expired authentication token
  **When** executing a gh command
  **Then** the error from gh is captured and surfaced to the user

### AC: Handle rate limit errors

- **Given** GitHub API rate limits are exceeded
  **When** executing a gh command
  **Then** the rate limit error from gh is captured and communicated

### AC: Handle network errors

- **Given** network connectivity issues
  **When** attempting to execute a gh command
  **Then** the network error is captured and reported appropriately

### AC: Parse JSON output from gh commands

- **Given** successful gh command execution with JSON output
  **When** parsing the JSON response
  **Then** structured data is extracted and made available for downstream processing

## Scope

### In Scope

- Shelling out to `gh` CLI for all GitHub API operations
- Executing `gh issue create`, `gh issue edit`, `gh issue list`, `gh issue view`, `gh issue close`, `gh issue reopen`
- Label management via `gh label create`
- Authentication validation via `gh auth status`
- JSON output parsing from gh commands
- Error handling for missing gh CLI, auth failures, rate limits, and network errors

### Out of Scope

- Native HTTP/reqwest client implementation
- Response caching or memoization
- Document format parsing
- Terminal UI integration
- gh CLI installation or configuration
