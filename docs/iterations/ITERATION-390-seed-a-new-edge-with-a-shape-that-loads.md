---
title: Seed a new edge with a shape that loads
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-260
---

## Objective

`n` on the Edges category appends an edge row that passes strict load unaided and drills into it; `d` removes one behind the existing confirm.

## Satisfies

STORY-260 AC6. AC1, AC2, AC5 landed in ITERATION-386, ITERATION-387 and ITERATION-388; AC4 lands in ITERATION-389 and AC3 in ITERATION-391.

## Context

- Story + ACs: STORY-260
- Touch:
  - `src/tui/state/app.rs:2683-2739` `settings_seed_entry` -- gains a `3 =>` arm where ITERATION-382 removed the rules one
  - `src/tui/state/app.rs:2795-2860` `settings_open_delete_confirm` and `:2861-2900` `settings_confirm_delete` -- the same
  - `README.md:199`
- **The seed cannot follow the pattern beside it.** The surviving arms seed placeholders that happen to load: `TypeDef { name: "type", prefix: "TYPE", .. }` (`:2686-2712`) and `RelationshipDef { name: "relationship", .. }` (`:2714-2721`) are safe because nothing at load cross-references them. The rules arm ITERATION-382 deleted seeded `child: String::new(), parent: String::new()` -- exactly the "empty row that fails to load" AC6 is written against. An edge *is* cross-referenced: strict load rejects an unknown type in `from`/`to` and an unknown relationship in `via` (`src/engine/config.rs:1327-1345`), so a placeholder string is a config that will not load.
- **Take the wildcard shape, and take its consequence.** `from = "*"`, `to = "*"`, `via = "*"`, no `traversal`, no `required` loads against any config, including one with a single relationship or none declared -- where reading the buffer for a real name would have nothing to read. Its consequence: ADR-031 §Consequences and RFC-067 §"The traversal cost, stated plainly" both say an all-wildcard row restores the blanket behaviour the table exists to escape. So the seed is a *shape that loads*, not a *row worth keeping*, and `required` must stay absent because ITERATION-370 rejects `required` on a wildcard `from`. State that where the seed lives, so nobody later "improves" the default by adding a severity.
- **Uniqueness is a workaround here, not a fix.** Nothing at load rejects two edges sharing a `name` (`config.rs:1307-1345`), and `write_edges` reconciles by `name` (ITERATION-377) -- so seeding twice with a constant name produces a config the loader accepts and the writer cannot address. Derive a name unique within the buffer. ITERATION-388 recorded the missing load check; this slice is the first place its absence would actually corrupt a file, so re-state it here rather than relying on the reader having read that slice.
- The delete path needs one arm and no design. `settings_open_delete_confirm`'s `3 =>` becomes an `edges.get(entry)` name lookup and `settings_confirm_delete`'s `3 =>` a `remove(i)` with the existing clamp. ADR-011's refusal to delete the last `[[relationships]]` entry (`app.rs:2812-2815`) has no analogue: a config with zero edges is legal and validates clean, it just constrains nothing.
- `README.md:199` still advertises `n` as seeding a "Validation Rules" entry -- a line ITERATION-382 removed the feature for and left standing. This is the slice that makes `n` mean something on this category, so it is the slice that corrects it.

## Tasks

1. Test-first: `settings_seed_entry` on category 3 appends one edge, and `Config::parse(&buffer.to_toml()?)` on the result is `Ok` -- once on a config with several relationships, once on a config with one. Then seed twice and assert the two names differ and the config still loads.
2. Add the seed arm with the wildcard shape and a buffer-unique name, ending with the same drill/field/dirty tail the other arms use (`app.rs:2735-2739`).
3. Add the delete arms, plus a test that deleting the only edge is permitted and leaves `edges` empty and the config loading.
4. Test seed-then-save end to end through ITERATION-388's writer: the row the writer emits is the row the buffer held. A seed whose validity is only asserted against `to_toml` proves nothing about the in-place writer, which is a second code path.
5. Test that the seeded row survives ITERATION-389's rejection path -- that is, a save straight after `n` succeeds rather than being refused. AC6's whole claim is that the seeded row does not need repairing first.
6. `README.md:199`: `n` seeds Document Types / Relationships / Edges.

## Out of scope

- Giving the loader a duplicate-name check. This slice works around its absence and records it; adding it is an engine change with no AC on this story.
- Seeding anything smarter than a wildcard row -- reading the buffer's first declared type pair, or offering a template. It would fail on a config the wildcard shape survives, which is the one thing AC6 asks for.
- The target-set picker (AC3) -> ITERATION-391. After this slice the seeded `to` is `"*"` and editing it goes through ITERATION-387's comma editor.
- `config add-edge` and the `init` wizard's edge loop -> STORY-261. ITERATION-383 removed the wizard's rule prompt and deliberately left no replacement; that gap is that story's, not this one's.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: the seed arm follows the two arms beside it in shape, and diverges only where cross-referencing forces it. The project instruction that a CLI/TUI change updates the README covers the key table.

## Verification

`lazyspec init` in an empty directory, open the TUI, `n` on the Edges category, `w`, then `lazyspec validate`: clean, and the written row is `from = "*"`, `to = "*"`, `via = "*"` with no `required` key. `n` twice then `w`: still clean, two distinctly named rows in the file.
