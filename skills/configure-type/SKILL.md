---
name: configure-type
description: Use when adding a new custom document type to a lazyspec project. Runs a grill-me-style interview to co-author the type's methodology -- intent, authorship, lifecycle, gates, relations -- then writes its enriched template and `[[types]]` config via the config-write CLI. One type per run.
---

```
ONE TYPE PER RUN. THE USER OWNS THE METHODOLOGY; YOU RECORD IT.
```

This skill configures exactly ONE document type per invocation. To add another
type, run `/configure-type` again. Keep the interview scoped to a single type --
if the user names a second type, finish the first, then tell them to re-run.

The engine ships no methodology for an arbitrary type (ADR-011). The only source
of per-type methodology is the user. So this is an interview: the user supplies
the knowledge, you extract it and record it as (1) an enriched template and (2) a
`[[types]]` config block written through the config-write CLI.

<NEVER>
- Do NOT open or hand-edit `.lazyspec.toml`. ALL config writes go through
  `lazyspec config add-type` / `config set-lifecycle` / `config add-gate`. The CLI
  preserves comments and formatting; a hand edit does not. This is non-negotiable.
- Do NOT configure more than one type per run.
- Do NOT ask for something `lazyspec config --json` can already tell you (existing
  type names, prefixes, dirs, relationship vocabulary, parent-child rules). Explore
  first, ask second.
- Do NOT invent lifecycle states or relations the user did not agree to. Recommend,
  then confirm.
</NEVER>

Always run `lazyspec help <subcommand>` or `--help` before using an unfamiliar
command, and pass `--json` when reading config. On failure, check `--help` and retry.

## Workflow

```d2
Read config --json -> Interview (one axis at a time) -> Confirm summary
Confirm summary -> Write enriched template (.lazyspec/templates/{name}.md)
Write enriched template -> config add-type -> config set-lifecycle -> config add-gate (if gated)
config add-gate (if gated) -> Verify with config --json -> Close out

Confirm summary.shape: diamond
Close out.shape: double_circle
```

## Preflight

Before asking anything, read the current config so you can recommend well and
avoid collisions:

```
lazyspec config --json | jq '{types: [.types[].name], prefixes: [.types[].prefix], dirs: [.types[].dir], relationships: .relationships, rules: .rules, templatesDir: .templates.dir}'
```

Use this to (a) reject a name/prefix/dir that collides, (b) enumerate existing
types the new one could relate to, and (c) find the templates dir to write into
(`.templates.dir`, default `.lazyspec/templates`).

## The interview

Ask the questions ONE AT A TIME. For each, give your recommended answer up front,
then let the user accept or override. Resolve the axes as a decision tree, in this
order -- later axes depend on earlier ones. If an answer is discoverable from
`config --json`, explore instead of asking.

1. **Name (and what falls out of it).** What is the type called (singular)?
   - From the name, propose the `plural`, `dir` (e.g. `docs/<plural>`), and `prefix`
     (uppercased, e.g. `SPIKE`). Recommend all three; the user confirms or tweaks.
   - Reject collisions against the names/prefixes/dirs you read in preflight.
   - Ask whether it is a `--singleton` (one document, not a numbered series) and the
     `--numbering` (`incremental` / `sqids` / `reserved`; default `incremental`) and
     `--store` (`filesystem` / `github-issues` / `git-ref`; default `filesystem`).
     Recommend `incremental` + `filesystem` unless the project pattern says otherwise.
   - Optional: an `--icon` for the TUI. Recommend one emoji.

2. **Intent.** One line: what is this document type FOR? This becomes both the
   template's `<!-- intent: ... -->` header AND the config `--intent`. Recommend a
   crisp single sentence in the user's words.

3. **Authorship ceiling.** `human`, `assisted`, or `generated` -- the highest
   authoring verb permitted for this type.
   - `human` -> only `/scaffold` (AI sets up the file, human writes the body).
   - `assisted` -> up to `/co-write` (AI drafts, human edits).
   - `generated` -> up to `/generate` (AI writes the full body).
   - Recommend based on intent: decision/judgement docs lean `human` or `assisted`;
     mechanical/derived docs can be `generated`.

4. **Lifecycle (per-type DAG).** Each type declares its own `states` and `edges`
   (ADR-021). Elicit:
   - **states** -- the statuses a document of this type moves through
     (e.g. `draft`, `review`, `done`). Recommend a minimal set.
   - **edges** -- permitted transitions as `FROM:TO`. `*` is allowed as the source
     to mean "from any state" (e.g. `*:superseded`). Recommend the minimal DAG that
     connects the states, plus a `*` edge to any terminal state like `rejected`/`superseded`.

5. **Parent + status gate.** Does a document of this type live UNDER a parent of an
   existing type (e.g. an iteration under a story)?
   - If yes, set `--parent-type <existing-type>` on `add-type`. This creates a
     parent-child rule.
   - THEN, and only then, ask the gate question: must the parent sit at a particular
     status before a child may be created? That is `require_parent_status`, set with
     `config add-gate`. `config add-gate` targets an existing parent-child RULE by its
     name -- read the rule name from `config --json` (`.rules[] | select(.shape=="parent-child")`)
     after `add-type` creates it. Skip this axis entirely if the type has no parent.

6. **Relations to existing types.** Beyond a parent, which existing types does this
   one link to, and with which relationship verb (e.g. `implements`, `related-to`)?
   - Enumerate the available relationship vocabulary from `config --json`
     (`.relationships[].name`) -- do not invent verbs.
   - The structural parent relation is expressed by `--parent-type`. Other relations
     are part of the project's relationship vocabulary and are applied to documents
     at authoring/link time, not baked per-type here; note any the user wants so the
     template guidance mentions them.

End the interview by reading back a one-screen summary of every axis and getting
explicit confirmation before writing anything.

## Write the enriched template

Write the per-type template to the configured templates dir (`.templates.dir` from
config, default `.lazyspec/templates/`), one file: `.lazyspec/templates/{name}.md`
(lowercased name). Use the enriched format from the shipped defaults: an
`<!-- intent: ... -->` header as the first body line, then one `## Section` per
logical part with a `<!-- guidance: ... -->` comment under each heading. Keep the
`{title}`/`{author}`/`{date}` placeholders verbatim; set `type:` to the literal
type name. The comments render invisibly -- they ARE the methodology, recorded as prose.

Ask the user what sections this type needs (recommend defaults from the intent).
Shape to mirror exactly:

```markdown
---
title: "{title}"
type: <name>
status: <first lifecycle state, e.g. draft>
author: "{author}"
date: {date}
tags: []
related: []
---
<!-- intent: <the one-line intent from the interview> -->

## <Section>
<!-- guidance: <what belongs in this section, in the user's words> -->

## <Section>
<!-- guidance: <...> -->
```

## Write the config (CLI only)

Write ALL config through these subcommands, in this order. Never touch the TOML by hand.

1. **Append the type:**
   ```
   lazyspec config add-type <name> <plural> <dir> <prefix> \
     --intent "<intent>" \
     --authorship <human|assisted|generated> \
     [--icon "<emoji>"] [--singleton] \
     [--numbering <incremental|sqids|reserved>] [--store <filesystem|github-issues|git-ref>] \
     [--parent-type <existing-type>]
   ```

2. **Set the lifecycle** (states + edges; `*` as source = any state):
   ```
   lazyspec config set-lifecycle <name> \
     --state <s1> --state <s2> [--state ...] \
     --edge <FROM:TO> --edge <FROM:TO> [--edge ...]
   ```

3. **Set the parent-status gate** (ONLY if the type has a parent and the user wanted a gate).
   `add-gate` gates an existing parent-child rule by its NAME -- get the rule name from
   `config --json` after step 1 created it:
   ```
   lazyspec config add-gate <rule-name> --status <required-parent-status>
   ```

## Close the loop

1. **Verify** the type landed with every axis populated:
   ```
   lazyspec config --json | jq '.types[] | select(.name=="<name>")'
   ```
   Confirm `intent`, `authorship`, and `lifecycle` (states + edges) match what was
   supplied. If a gate was set, confirm the rule's `require_parent_status`:
   ```
   lazyspec config --json | jq '.rules[] | select(.name=="<rule-name>")'
   ```

2. **Verification checklist** -- all must hold:
   - [ ] `.lazyspec/templates/{name}.md` written, with an `<!-- intent: ... -->`
         header and a `<!-- guidance: ... -->` comment per section.
   - [ ] The type appears in `config --json` with `intent`, `authorship`, and
         `lifecycle` (states + edges) populated as supplied.
   - [ ] If parented + gated: the parent-child rule shows the expected
         `require_parent_status`.
   - [ ] No direct edit of `.lazyspec.toml` occurred -- every write went through `config`.
   - [ ] Exactly ONE type was configured.

3. **Reiterate:** to add another type, run `/configure-type` again. This skill does
   one type per run.

## Rules

- One type per run. Configuring more types means re-running the skill.
- All config writes go through `config add-type` / `set-lifecycle` / `add-gate`;
  never hand-edit `.lazyspec.toml`.
- Explore `config --json` before asking; recommend an answer for every question.
- Each type carries its own inline lifecycle DAG (ADR-021).
- The user owns the methodology; this skill only records it (ADR-011, ADR-019).
