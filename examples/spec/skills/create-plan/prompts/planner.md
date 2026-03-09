# Planner Subagent Prompt

Include the subagent preamble from `_common.md`, then append:

```
You are creating a single Plan document within the lazyspec workflow.

## Spec Context
[Full Spec body]

## RFC Design Intent
[1-2 paragraphs from the RFC, if one exists]

## Your Contract Group
[List of contracts this plan covers]

## Other Contract Groups (for boundary awareness)
[List of other groups and their contract assignments]

## Instructions
1. Create the plan: `lazyspec create plan "<title>" --author agent`
2. Link to spec: `lazyspec link <plan-path> implements <spec-path>`
3. Discover relevant code using `lazyspec search` and Explore subagents
4. Plan tests for each contract in your group
5. Write task breakdown with real, verified file paths
6. Validate: `lazyspec validate --json`
7. Report: plan path, contracts covered, task count, any concerns
```
