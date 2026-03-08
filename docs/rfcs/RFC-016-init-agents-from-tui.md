---
title: Init agents from tui
type: rfc
status: draft
author: "@jkaloger"
date: 2026-03-08
tags:
  - tui
  - agents
  - ai
---

## Summary

Allow invoking agents from the tui

This would instantiate an agent in headless mode from the tui to

- Auto complete an idea sketch/feature idea from an rfc
- Auto create all stories from a rfc
- Auto create implementations from stories

Allow iterating on rfcs,stories,implementations

- A view of all current/past agents is visible in a new screen
- This would show all agents invoked from the tui
- Users can open the agent view, similar to $EDITOR

Support for Claude only to start with, need to build in support for other agents

A workflow might look like;

- select an rfc from docuemnts list
- `a` to invoke agent with dialog
  - expand document
  - create stories/iterations from rfc/story (this must be derived from config i.e. parent child rules)
  - custom prompt

Custom prompts need to be picked up from .lazyspec/agents/
