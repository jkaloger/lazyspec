---
title: Document Context Chain
type: spec
status: draft
author: jkaloger
date: 2026-03-25
tags:
- cli
- context
- relationships
related:
- related-to: STORY-019
- related-to: STORY-054
---


## Summary

The `lazyspec context` command resolves a document's full lineage by walking `implements` relationships upward and collecting forward implementors and related documents. The result is a `ResolvedContext` (@ref src/cli/context.rs#ResolvedContext) containing the backward chain, target position, forward children, and related records. Two output modes render this structure for human and machine consumers.

## Chain Resolution

`resolve_chain` (@ref src/cli/context.rs#resolve_chain) accepts a `Store` and a shorthand ID string. It first calls `resolve_shorthand` (@ref src/engine/store.rs#resolve_shorthand) to locate the target document, mapping `ResolveError` variants into `anyhow` errors. For `NotFound`, the message is `"document not found: {id}"`. For `Ambiguous`, the message lists all matching paths and prompts the user to specify the full path.

From the resolved document, the function walks backward by repeatedly inspecting the current chain head's `related` entries for an `Implements` relation. When one is found, its `target` path is looked up in the store via `Store::get`, and the parent is prepended to the chain vector. The loop terminates when no `Implements` relation exists on the current head, meaning the chain root has been reached.

The `target_index` field records the position of the originally-requested document within the chain, so renderers can mark it distinctly.

## Forward Context

After building the backward chain, `resolve_chain` collects forward context: documents that implement the target. It queries `Store.reverse_links` (@ref src/engine/store.rs#Store) for the target's path, filters to entries where the relation type is `Implements`, and resolves each source path through the store. Only direct implementors of the target are included; the traversal does not recurse transitively.

## Related Records

Related records are gathered from every document in the chain, not just the target. For each chain member, the function examines both `Store.forward_links` and `Store.reverse_links`, selecting entries with `RelationType::RelatedTo` (@ref src/engine/document.rs#RelationType). Documents already present in the chain are excluded. A `HashSet` tracks seen paths to prevent duplicates when multiple chain members link to the same related document.

## Human Output

`run_human` (@ref src/cli/context.rs#run_human) renders the resolved context as a vertical chain of mini-cards connected by tree-drawing characters.

Each chain member is rendered by `mini_card` (@ref src/cli/context.rs#mini_card), which draws a bordered box containing the document title and a line with the uppercased shorthand ID, lowercase doc type, and status in brackets. When `colors_enabled()` returns true, the box uses Unicode box-drawing characters and the status receives colour styling via `styled_status`. When colours are disabled, the box falls back to ASCII (`+`, `-`, `|`).

The target document's mini-card receives a `"<- you are here"` marker appended to its title line, controlled by comparing the loop index against `target_index`.

Between chain cards, `chain_connector` (@ref src/cli/context.rs#chain_connector) prints a vertical pipe character. Below each chain card, `run_human` calls `Store::children_of` to list child documents as indented lines with tree connectors (`|-` and `\-`), showing each child's shorthand, title, and status.

After the final chain card, forward implementors are rendered in the same tree-connector style, preceded by a chain connector. If the forward list is empty, this section is omitted entirely.

Related records appear after a blank line and a `"--- related ---"` separator (or its Unicode equivalent when colours are enabled). Each related document is printed as `SHORTHAND  Title [status]`. The section is omitted when there are no related records.

## JSON Output

`run_json` (@ref src/cli/context.rs#run_json) serializes the resolved context into a JSON object with two top-level keys: `chain` and `related`. Each element in both arrays is produced by `doc_to_json_with_family` (@ref src/cli/json.rs#doc_to_json_with_family), which includes full frontmatter fields plus any children and parent information. Forward implementors are not surfaced as a separate JSON field; they appear only in the human output.
