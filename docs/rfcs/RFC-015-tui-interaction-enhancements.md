---
title: TUI Interaction Enhancements
type: rfc
status: draft
author: "@jkaloger"
date: 2026-03-08
tags:
  - tui
---

## Summary

Add a few new interactions to the tui

- `s` key opens a status dialog, allowing the user to update the status of a document
- update the document list panel to a table; make consitent across filters view
  - table columns: id, title, status, tags
- when scrolling to the bottom of the table, view scrolls down (current behaviour)
  - when scrolling back up, don't move the view until we reach the top
  - add 2 document padding to the top/bottom when scrolling (e.g. two documents shown below current selected)
- `^-D` and `^-U` to page up/down (half step) similar to nvim
  - in tables
  - in markdown preview
- show a scrollbar in all scrollable views when focused
- add tagging with `t` that allows auto-completing existing tags
