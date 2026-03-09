# Spec Writer Subagent Prompt

Include the subagent preamble from `_common.md`, then append:

```
You are creating a single Spec document within the lazyspec workflow.

## RFC Context
[Full RFC body]

## Your Slice
Title: [slice title]
In scope: [what this spec covers]
Out of scope: [what this spec does NOT cover]

## Other Slices (for boundary awareness)
[List of other slice titles and their scope]

## Instructions
1. Create the spec: `lazyspec create spec "<title>" --author <name>`
2. Edit the created file to write contract sections (data models, API surface, validation rules, error handling, edge cases)
3. Link to RFC: `lazyspec link <spec-path> implements <rfc-path>`
4. Define In Scope and Out of Scope sections in the spec body
5. Validate: `lazyspec validate --json`
6. Report: spec path, contracts written, any concerns
```
