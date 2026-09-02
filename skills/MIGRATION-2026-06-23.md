# Migrating to the generic verb skills

_2026-06-23 — see RFC-048 (Config-driven agent skills and workflow)_

The skill set changed shape. The old skills were hand-authored markdown hard-coupled to the `RFC → Story → Iteration` chain: each one baked a type name into its prose. RFC-048 replaced them with a small, static, DAG-agnostic verb set that reads `lazyspec config --json` at runtime and takes the document _type_ as a parameter. The same prose now works for any configured DAG, not just the shipped default.

If you have a custom DAG (anything other than `rfc`/`story`/`iteration`), this is the change that makes the skills know your types exist.

## Install the new skills

```
lazyspec skills install                  # both runtimes (default)
lazyspec skills install --runtime claude     # .claude/skills/ only
lazyspec skills install --runtime agents-md  # ./AGENTS.md only
```

`npx skills` and `lazyspec skills install` place the identical skill source. The skill prose is portable: a Claude `SKILL.md` and an `AGENTS.md` differ only by frontmatter and filename, not content.

## Remove the old skills

Delete the old per-type skills from `.claude/skills/`. They no longer have matching CLI behaviour and will compete with the router:

```
write-rfc  create-story  create-iteration  build  review-iteration  plan-work  resolve-context
```

## What maps to what

| Old skill          | New skill                             | Notes                                                                                                                                  |
| ------------------ | ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `plan-work`        | `/lazy`                               | Entry router. Reads the DAG + your position, dispatches the right verb, stops at type boundaries (the `chain` rows in `edges`).        |
| `write-rfc`        | `/scaffold`, `/co-write`, `/generate` | Authoring is now three ceiling-ordered verbs (`scaffold < co-write < generate`); the type (`rfc`) is a parameter.                      |
| `create-story`     | `/scaffold`, `/co-write`, `/generate` | Same authoring verbs, `type = story`. The per-type intent and section guidance live in the type's template, not the skill.             |
| `create-iteration` | `/scaffold`, `/co-write`, `/generate` | Same authoring verbs, `type = iteration`.                                                                                              |
| `build`            | `/execute`                            | Carry out the work a delivery doc describes against its task breakdown and ACs. No authorship ceiling — this is work, not authoring.   |
| `review-iteration` | `/review`                             | Two-stage critique: conformance to intent + ACs first, quality second. Type-agnostic.                                                  |
| `resolve-context`  | _(removed)_                           | Folded into `lazyspec context --json`. `/lazy` reads the chain from the CLI instead of calling a skill.                                |
| _(none)_           | `/advance`                            | New. Move a doc to its next lifecycle status. Status only — never spawns children.                                                     |
| `create-audit`     | `create-audit`                        | Unchanged. Still runs independently of the main pipeline.                                                                              |
| _(none)_           | `/configure-type`                     | New. Grill-me-style interview to author one custom type's intent, authorship, lifecycle, and template via the config-write CLI.        |

## Behaviour changes to know about

Three shifts come with the new skills, all driven by config rather than baked into prose:

**Authorship is a ceiling.** Each type declares `authorship` in config (`human`, `assisted`, `generated`). It caps the highest authoring verb permitted: `human` → `/scaffold` only, `assisted` → up to `/co-write`, `generated` → up to `/generate`. A verb above the ceiling is refused; anything at or below is always allowed, so you can pick a more manual mode on any type. The old skills had no such ceiling and pushed toward generating.

**The router stops at type boundaries.** `/lazy` advances within the current document automatically, but never auto-creates a child of a different type. Crossing a type boundary is always human-initiated. This is the planning→delivery handoff the old build-eager skills lacked. A boundary is a `traversal: chain` row in the config's `edges` table and nothing else — `parent_type` declares none.

**Status is a per-type DAG.** Status is now a validated string over each type's declared `lifecycle` (states + edges). `update --status` rejects any transition not on a declared edge. Run `fix --config` on a pre-RFC-048 project to inject the default lifecycle DAG into the existing config.

## After installing

Re-materialize the enriched templates so the per-type intent and section guidance (`<!-- intent: -->` / `<!-- guidance: -->` comments) land on disk:

```
lazyspec init
```

Then drive everything through `/lazy` — it routes from your current position in the DAG.
