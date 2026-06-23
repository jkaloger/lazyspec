---
name: configure-type
description: Use when adding a new custom document type to a lazyspec project. Interviews the user to co-author the type's methodology -- intent, authorship, lifecycle, gates, relations -- then writes its enriched template and `[[types]]` config via the config-write CLI. One type per run.
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

Interview the user relentlessly about every aspect of the type until you reach a
shared understanding, walking down each branch of the design tree and resolving
dependencies between decisions one-by-one. For each question, provide your
recommended answer up front, then let the user accept or override. Ask the
questions ONE AT A TIME. If a question can be answered by exploring the codebase
or `config --json`, explore instead of asking.

The interview is open-ended, but it is not done until you have resolved every
field below -- the config-write CLI fails if any required field is missing, and
the recommended values are the legal value sets, not suggestions you may widen.
Resolve them roughly in this order; later fields depend on earlier ones.

| Field | Required | Legal values / shape | Notes |
|---|---|---|---|
| `name` | yes | singular noun | Reject collisions vs preflight names. |
| `plural`, `dir`, `prefix` | yes | `dir` e.g. `docs/<plural>`; `prefix` uppercased e.g. `SPIKE` | Derive all three from `name`; reject collisions. |
| `--singleton` | no | flag | One document, not a numbered series. |
| `--numbering` | no | `incremental` \| `sqids` \| `reserved` | Default `incremental`. |
| `--store` | no | `filesystem` \| `github-issues` \| `git-ref` | Default `filesystem`. |
| `--icon` | no | one emoji | For the TUI. |
| `--intent` | yes | one sentence | Becomes BOTH the template `<!-- intent: -->` header and config `--intent`. |
| `--authorship` | yes | `human` \| `assisted` \| `generated` | Highest authoring verb: `human`→`/scaffold`, `assisted`→`/co-write`, `generated`→`/generate`. Decision/judgement docs lean `human`/`assisted`; mechanical/derived can be `generated`. |
| lifecycle `states` | yes | list of statuses, e.g. `draft`,`review`,`done` | Per-type DAG (ADR-021). Recommend a minimal set. |
| lifecycle `edges` | yes | `FROM:TO`; `*` as source = "from any state" | Minimal DAG connecting the states, plus a `*` edge to any terminal state (`rejected`/`superseded`). |
| `--parent-type` | no | an existing type name | Does this live UNDER a parent (e.g. iteration under story)? Creates a parent-child rule. |
| parent-status gate | no | a parent status (`require_parent_status`) | ONLY ask if `--parent-type` is set. Must the parent sit at a status before a child may be created? Set via `config add-gate`, which targets the parent-child RULE by name -- read the name from `config --json` (`.rules[] \| select(.shape=="parent-child")`) AFTER `add-type` creates it. |
| relations | no | a verb from `config --json` `.relationships[].name` | Do NOT invent verbs. The parent relation is `--parent-type`; other relations are applied at authoring time, not baked per-type -- note any the user wants so the template guidance mentions them. |

End the interview by reading back a one-screen summary of every field and getting
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
