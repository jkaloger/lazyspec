# openspec pack

A lazyspec workflow pack matching [OpenSpec](https://github.com/Fission-AI/OpenSpec)'s document structure: capability `spec`s as the source of truth under `openspec/specs/`, and each `change` as a folder of `proposal.md`, `design.md` and `tasks.md` under `openspec/changes/`, with one `delta` spec per capability the change affects. Section headings, requirement and scenario formats are OpenSpec's own, so documents written here read the same as in an `openspec` project.

## Layout

| Path | Role |
| --- | --- |
| `.lazyspec.toml` | The pack's config: three types (`spec`, `change`, `delta`), the `implements` relationship, and one edge nudging a delta to link back to its change. |
| `.lazyspec/templates/spec.md` | The flat template for a capability `spec`: `# <capability> Specification`, `## Purpose`, `## Requirements`. |
| `.lazyspec/templates/change/` | A directory template. `index.md` is the change document itself; `proposal.md`, `design.md` and `tasks.md` are its parts. |
| `.lazyspec/templates/delta.md` | The flat template for a `delta` spec: `# Spec Delta` with `## ADDED Requirements` and friends. |
| `README.md` | This file. |

## Adoption

`lazyspec init --template <path-to-this-directory>` copies `.lazyspec.toml` and `.lazyspec/templates/` into the current project. `--force` overwrites an existing `.lazyspec.toml`; without it, `init` refuses when one is already present. Document storage stays local: adopting this pack sets no `extends` and moves nothing.

## Documents

A `spec` is created per capability with `lazyspec create spec <capability>`, landing at `openspec/specs/SPEC-NNN-<capability>.md`. It holds the capability's `## Purpose` and its `## Requirements`, each `### Requirement:` carrying at least one `#### Scenario:` with WHEN/THEN bullets.

A `change` is created with `lazyspec create change <title>`, which scaffolds `openspec/changes/CHANGE-NNN-<slug>/index.md`, `proposal.md`, `design.md` and `tasks.md` in one call. `index.md` is lazyspec's header for the bundle (frontmatter and a one-paragraph summary); the other three are OpenSpec's artifacts verbatim: `proposal.md` has `## Why`, `## What Changes`, `## Capabilities` (`### New Capabilities` / `### Modified Capabilities`) and `## Impact`; `design.md` has `## Context`, `## Goals / Non-Goals`, `## Decisions` and `## Risks / Trade-offs`; `tasks.md` is numbered task groups of `- [ ] X.Y` checkboxes.

A `delta` is created per capability the proposal names with `lazyspec create delta <capability> --parent <change-id>`, which lands it as a sibling file inside the change's own folder. It is OpenSpec's delta spec: `## ADDED Requirements`, `## MODIFIED Requirements`, `## REMOVED Requirements` and `## RENAMED Requirements` sections, each requirement with its scenarios.

The `deltas-need-changes` edge reports a warning when a delta carries no `implements` link back to its change; it does not block `create` or `validate`.

## Where this differs from OpenSpec

- **Delta location.** OpenSpec nests delta specs at `changes/<name>/specs/<capability>/spec.md`. lazyspec keeps children one level under their parent, so a delta lands at `changes/CHANGE-NNN-<slug>/DELTA-NNN-<capability>.md` instead.
- **File names and IDs.** OpenSpec names folders by slug alone (`changes/add-dark-mode/`, `specs/user-auth/spec.md`); lazyspec prefixes them with a numbered ID and keeps a flat `SPEC-NNN-<capability>.md` per capability.
- **`index.md` and frontmatter.** OpenSpec carries no frontmatter and no index file; lazyspec's bundle header holds the change's status and relations.
- **No `.openspec.yaml` and no `archive`.** Status lives in frontmatter (`draft` → `approved` → `archived`), and folding a delta into its main spec is a manual edit rather than an `openspec archive` step.
