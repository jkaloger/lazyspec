---
title: "update --body omits the frontmatter separator newline"
type: bug
status: in-progress
author: "Jack Kaloger"
date: 2026-08-29
tags: []
related: []
---


## Expected

`lazyspec update <ID> --body` / `--body-file` writes a document whose body is separated from the closing frontmatter delimiter, matching what `create --body` produces:

```
---
title: ...
---

## Problem
```

## Actual

The body is concatenated directly onto the delimiter, so the first line of the body is swallowed into it and does not render as markdown:

```
---
title: ...
---## Problem
```

## Repro

```bash
lazyspec create rfc "Sep test" --body-file body.md   # correct: "---\n\n## Problem"
lazyspec update RFC-0NN --body-file body.md          # wrong:   "---## Problem"
```

Same input file, two different outputs. Observed on RFC-067 during RFC authoring; worked around by prepending a newline to the body file.

## Root cause

Two write paths that do not agree on who owns the separator.

`create` composes explicitly at `src/engine/fs_ops.rs:250`:

```rust
let new_content = format!("---\n{}\n---\n\n{}\n", yaml.trim(), body_text);
```

`update` replaces the body wholesale at `src/engine/fs_ops.rs:337-341` and then hands it to `compose_frontmatter` at `src/engine/fs_ops.rs:384`:

```rust
let mut new_body = body;              // from split_frontmatter -- carries its leading \n
if *key == "body" {
    new_body = value.to_string();     // from CLI -- does not
}
let new_content = compose_frontmatter(&new_yaml, &new_body);
```

`compose_frontmatter` (`src/engine/document.rs:376-382`) is byte-faithful by contract — its doc comment states the body is "preserved byte-for-byte (including any leading newline that follows the closing `---` delimiter), so repeated split/compose cycles do not accumulate blank lines". That contract is correct for round-tripping a body that came from `split_frontmatter`. It is wrong for a body that came from the CLI, which has no leading newline.

So the defect is not in `compose_frontmatter`. It is that `update` substitutes a differently-shaped value into a slot whose invariant it does not restore.

## Notes for the fix

Normalising inside `compose_frontmatter` would break its round-trip guarantee and risk stripping intentional leading blank lines. The substitution site is the right place: `update` should restore the leading-newline invariant when it replaces the body from an external source, leaving the split/compose round-trip untouched.

Worth checking whether the same substitution shape appears in the `github-issues`, `clickup-tasks`, and `git-ref` update paths, or whether it is filesystem-only.

