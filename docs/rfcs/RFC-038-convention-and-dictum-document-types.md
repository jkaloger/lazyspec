---
title: Convention and Dictum Document Types
type: rfc
status: accepted
author: jkaloger
date: 2026-03-26
tags:
- convention
- dictum
- config
- types
related:
- related-to: RFC-013
- related-to: RFC-014
---



## Summary

Introduce two new document types: convention (a singleton project manifesto) and dictum (tagged principles that live as children of the convention). Convention captures the project's constitution. Dictum capture specific principles such as testing philosophy, module architecture, or trait implementation patterns.

A hybrid context-surfacing model loads the convention preamble at agent boot and pulls relevant dictum selectively during skill execution via tags.

## Problem

Lazyspec manages structured documents through a workflow pipeline (RFC, Story, Iteration), but has no mechanism for project-wide principles that should inform _all_ work. Teams develop conventions around testing, architecture, error handling, and code organisation that currently live in scattered CLAUDE.md files, READMEs, or tribal knowledge.

Without a structured home for these principles, agents rediscover or contradict them across conversations. A new contributor (human or agent) has no single place to understand the project's values and constraints.

## Design

### Two new types

Convention is a singleton document type. One per project, living as a folder-based document:

```
docs/convention/
  index.md          # the convention itself (preamble, project values)
  testing.md        # dictum: testing philosophy
  architecture.md   # dictum: module architecture principles
  error-handling.md # dictum: error handling approach
```

Dictum are children of the convention folder. Each dictum is a full document with its own frontmatter, tags, and content. The folder containment relationship (RFC-014) handles the parent-child link implicitly.

### Type configuration

Two new entries in `.lazyspec.toml`, extending the existing `[[types]]` array:

```toml
[[types]]
name = "convention"
plural = "convention"
dir = "docs/convention"
prefix = "CONVENTION"
icon = "📜"
singleton = true

[[types]]
name = "dictum"
plural = "dicta"
dir = "docs/convention"
prefix = "DICTUM"
icon = "⚖"
parent_type = "convention"
```

### Singleton constraint

A new optional field on `TypeDef`:

```toml
singleton = true   # default: false
```

@ref src/engine/config.rs#TypeDef

When `singleton` is true:

- `lazyspec create convention` succeeds the first time, creating `docs/convention/index.md`
- A second `lazyspec create convention` fails with an error: "convention already exists at docs/convention/index.md"
- Validation emits an error if more than one document of a singleton type is discovered

@ref src/cli/create.rs#run

The create command checks the store for existing documents of the type before proceeding. This is a pre-creation guard, not just a validation-time check.

@draft TypeDef {
    name: String,
    plural: String,
    dir: String,
    prefix: String,
    icon: Option<String>,
    numbering: NumberingStrategy,
    singleton: bool,        // NEW: at most one document of this type
    parent_type: Option<String>, // NEW: must be a child of this type's folder
}

### Parent type constraint

A new optional field on `TypeDef`:

```toml
parent_type = "convention"
```

When `parent_type` is set:

- Documents of this type can only exist as children within the parent type's folder
- `lazyspec create dictum "Testing Philosophy"` creates a child document inside `docs/convention/` (the convention's directory)
- Validation emits an error if a dictum document is found outside the convention folder
- The parent type must be a singleton (it wouldn't make sense to constrain children to an ambiguous parent)

### CLI: `lazyspec convention` subcommand

A dedicated subcommand for reading convention and dictum content, optimised for agent consumption:

```
lazyspec convention                    # full convention + all dictum
lazyspec convention --preamble         # convention index.md only (for boot)
lazyspec convention --tags testing     # dictum matching tag "testing"
lazyspec convention --tags "testing,architecture"  # multiple tags (OR)
lazyspec convention --json             # structured output
```

Output format (non-JSON) is the convention preamble followed by each matching dictum, separated by headings. The command reads from the store like any other query, no special file handling.

@draft ConventionCommand {
    preamble: bool,         // only show convention index.md
    tags: Option<Vec<String>>, // filter dictum by tags
    json: bool,
}

### Context surfacing: hybrid model

Two integration points bring convention and dictum into agent context:

**Boot hook (convention preamble).** A Claude Code `user-prompt-submit` hook runs `lazyspec convention --preamble` and injects the output at session start. This gives the agent the project's constitution from the first message. The preamble should be kept short (a few paragraphs) to minimise token overhead.

```json
{
  "hooks": {
    "user-prompt-submit": [
      {
        "command": "lazyspec convention --preamble",
        "type": "intercept"
      }
    ]
  }
}
```

**Skill-time dictum.** Skills like `/write-rfc`, `/build`, and `/create-iteration` call `lazyspec convention --tags <relevant-tags> --json` during their preflight phase to pull dictum that apply to the current work. The skill knows its domain and selects appropriate tags. This is surgical: a testing-focused iteration pulls testing dictum, not architecture dictum.

### Default content

`lazyspec init` ships a minimal convention skeleton when the type is configured:

**`docs/convention/index.md`** (convention preamble):

```markdown
---
title: "Convention"
type: convention
status: draft
author: "unknown"
date: {date}
tags: []
---

This is your project's convention. It captures the values, constraints, and
principles that should inform all work in this repository.

Edit this document to describe your project's constitution. Keep it short.
Dictum (child documents in this folder) capture specific principles.
```

**`docs/convention/example.md`** (example dictum):

```markdown
---
title: "Example Dictum"
type: dictum
status: draft
author: "unknown"
date: {date}
tags: [example]
---

This is an example dictum. Replace it with a principle that matters to your project.

Each dictum should cover a single topic and be tagged for selective retrieval
by agent skills. For example, a dictum about testing philosophy would have
`tags: [testing]`.
```

### Validation

Two new validation concerns, handled within the existing validation pipeline:

1. Singleton violation: more than one document of a singleton type exists
2. Parent type violation: a document with `parent_type` configured exists outside the parent's directory

These map naturally to new validation error variants, not new rule shapes. They're structural constraints on the type system, not relationship rules.

@ref src/engine/validation.rs

## Stories

1. Singleton and parent type constraints (engine): add `singleton` and `parent_type` fields to `TypeDef`, enforce in create and validation
2. Convention CLI subcommand: `lazyspec convention` with `--preamble`, `--tags`, `--json` flags
3. Default convention/dictum content: skeleton files created by `lazyspec init` when convention type is configured
4. Skill and hook integration: update skills to pull dictum during preflight, configure boot hook for convention preamble
