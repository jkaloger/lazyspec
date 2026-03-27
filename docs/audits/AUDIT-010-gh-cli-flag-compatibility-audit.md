---
title: GH CLI flag compatibility audit
type: audit
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- related-to: STORY-097
- related-to: RFC-037
---



## Scope

Bug bash of the `GhCli` implementation in `src/engine/gh.rs` against the actual `gh` CLI interface. Triggered by a runtime failure: `gh issue create` rejects the `--json` flag, but lazyspec passes it unconditionally.

## Criteria

Each `gh` subcommand invocation in `GhCli` must use only flags that the corresponding `gh` CLI subcommand actually supports. Error classification must not produce false positives from unrelated text in stderr.

## Findings

### Finding 1: `issue_create` passes unsupported `--json` flag

- Severity: critical
- Location: `src/engine/gh.rs:261`
- Description: `GhCli::issue_create` appends `--json number,url,title,body,labels,state,updatedAt` to the `gh issue create` invocation. The `gh issue create` subcommand does not support `--json`. It outputs the created issue URL to stdout on success, not JSON. This causes every issue creation to fail.
- Recommendation: Remove the `--json` flag from the `issue_create` args. Parse the issue URL from stdout instead, or use a follow-up `gh issue view --json` call to retrieve structured data after creation.

### Finding 2: `classify_error` misclassifies usage errors as auth failures

- Severity: high
- Location: `src/engine/gh.rs:84`
- Description: When `gh issue create` fails due to `--json` being an unknown flag, gh prints the full usage text to stderr. That usage text includes the phrase "Assign people by their login", which contains the substring "login". The `classify_error` function (line 84) checks `lower.contains("login")` and matches, producing `GhError::AuthFailure` for what is actually a flag validation error. The user sees "gh auth failure: unknown flag: --json" which is misleading.
- Recommendation: Make the auth-detection patterns more specific. Matching on "login" alone is too broad. Consider matching on phrases like "gh auth login", "not logged in", or "authentication" instead of bare "login".

### Finding 3: `issue_create` return type assumes JSON parsing

- Severity: medium
- Location: `src/engine/gh.rs:247-265`
- Description: `issue_create` returns `Result<GhIssue>` and calls `parse_issue_json(&stdout)`. Even after removing `--json`, the stdout from `gh issue create` is a URL string (e.g. `https://github.com/owner/repo/issues/42`), not JSON. The parse will fail with a JSON deserialization error.
- Recommendation: Either extract the issue number from the returned URL and call `issue_view` to get full JSON, or change the return type to something like `Result<u64>` (the issue number) and let callers fetch details separately.

### Finding 4: `issue_edit` uses `--title` but `gh issue edit` uses `-t/--title`

- Severity: info
- Location: `src/engine/gh.rs:280`
- Description: The long-form `--title` flag is used, which is correct. Both `-t` and `--title` are valid for `gh issue edit`. No issue here, noting for completeness.
- Recommendation: None.

### Finding 5: `label_create` and `label_ensure` flag usage is correct

- Severity: info
- Location: `src/engine/gh.rs:379-420`
- Description: `label_create` uses `--description` and `--color`, `label_ensure` adds `--force`. All match the `gh label create` interface exactly.
- Recommendation: None.

### Finding 6: `issue_list`, `issue_view`, `issue_close`, `issue_reopen` flag usage is correct

- Severity: info
- Location: `src/engine/gh.rs:300-350`
- Description: These subcommands use `--json`, `--repo`, `--label` in ways that match the actual `gh` CLI. `gh issue list` and `gh issue view` both support `--json`.
- Recommendation: None.

## Summary

Two actionable bugs. The critical one (`--json` on `issue_create`) breaks all issue creation. The high-severity one (`classify_error` matching "login" in usage text) masks the real error with a misleading auth failure message. The medium one (JSON parsing of non-JSON stdout) will surface once the `--json` flag is removed. The three together form a single fix chain: remove the flag, fix the parse strategy, tighten the error classifier.
