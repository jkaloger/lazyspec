---
title: "Convention"
type: convention
status: accepted
author: "jkaloger"
date: 2026-03-27
tags: []
---


Lazyspec is a structured document management tool for software projects. It manages RFCs, stories, iterations, ADRs, and other document types through a CLI and TUI, with an engine that handles validation, relationships, and content resolution.

The project dogfoods itself: lazyspec documents are managed by lazyspec. All feature work follows the RFC, Story, Iteration pipeline using the tool's own skills and CLI.

The codebase is Rust, organized into three layers: `engine` (pure domain logic), `cli` (thin command handlers), and `tui` (interactive terminal UI). The engine owns all business rules. The CLI and TUI are consumers of the engine, never the other way around.

Dictum in this folder capture specific principles. Each is tagged for selective retrieval by agent skills during their preflight phase.
