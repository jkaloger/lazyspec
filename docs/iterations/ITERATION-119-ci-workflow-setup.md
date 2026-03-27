---
title: CI workflow setup
type: iteration
status: draft
author: agent
date: 2026-03-27
tags: []
related:
- implements: STORY-094
---


## Changes

### Task 1: Create CI workflow

ACs addressed: AC-1 through AC-8 (all)

Files:
- Create: `.github/workflows/ci.yml`

What to implement:

A GitHub Actions workflow with trigger on `pull_request` (branches: main) and `push` (branches: main). Three jobs, all on `ubuntu-latest`:

1. `check` -- installs Nix via DeterminateSystems/nix-installer-action, enables magic-nix-cache-action, runs `nix flake check`
2. `build` -- same Nix setup, runs `nix build`
3. `validate` -- same Nix setup, runs `nix develop --command cargo run -- validate`

## Test Plan

Push the branch and open a PR to verify the workflow triggers and runs.

## Notes

Single-task iteration. The workflow is a direct translation of the RFC-024 CI design.
