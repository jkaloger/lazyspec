---
name: resolve-context
description: Use when an agent needs full context before beginning work on a Spec or Plan. Gathers the document chain from plan through spec to RFC.
---

```
NO IMPLEMENTATION WITHOUT FULL CONTEXT
```

If you haven't read the Spec -> existing Plan chain (and RFC if one exists), you cannot write code.

<HARD-GATE>
Do NOT begin implementation without completing this skill. Read the full
document chain (Spec -> Plan, and RFC if linked) before writing any code.
After completion: use the `/create-plan` skill.
</HARD-GATE>

> [!IMPORTANT]
> Read `_common.md` in the skills directory for CLI usage, forbidden actions, and subagent tiers.

# Resolve Context

## Workflow Position

```d2
lazy -> write-rfc -> create-spec -> resolve-context -> create-plan -> build

resolve-context.style.fill: "#4A9EFF"
resolve-context.style.font-color: "#FFFFFF"
lazy.style.opacity: 0.4
write-rfc.style.opacity: 0.4
create-spec.style.opacity: 0.4
create-lazy.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
Identify target doc -> Resolve chain -> Read bodies if needed -> Check for existing work -> State context back -> Context complete

Context complete.shape: double_circle
```

## Preflight

1. Identify the target document path or ID
2. Confirm the document exists with `lazyspec show <id> --json`

## Subagent Dispatch

| Operation | Agent Type | Tier | Context to provide |
|-----------|-----------|------|-------------------|
| Discover relevant codebase files | Explore | Medium | Type names, module paths from spec documents |
| Summarize context | _(inline)_ | - | Main agent synthesizes findings |

## Steps

1. **Identify the document:** Use `lazyspec list --json` or `lazyspec search <query> --json` to find the target document.

2. **Resolve the chain:** Run `lazyspec context <id> --json` to get the full implements chain (Spec -> Plan, and RFC if linked) in one call.

3. **Read document bodies:** The context command shows frontmatter only. For documents where you need the full body (typically the Spec contracts and RFC design intent if an RFC exists), follow up with `lazyspec show <id> --json` on those specific documents.

4. **Check for existing work:** Run `lazyspec status --json` to get all documents, relationships, and validation results in one call. Look for existing plans, ADRs, or related documents that cover the same ground.

5. **Discover relevant code:** The spec documents often name exact files and symbols -- use those as starting points rather than guessing at file paths.

6. **Assemble context:** You now have the full chain: Spec (contracts) -> existing Plans (prior work) -> relevant codebase locations. If an RFC exists: RFC (intent) -> Spec -> Plans.

7. **State it back:** Before proceeding, summarise the context chain: what the RFC intends (if one exists), what contracts the Spec defines, what prior plans have already done, and which parts of the codebase are involved. This forces you to confirm you actually understood it.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I already know this codebase" | Knowledge decays. Prior plans may have changed assumptions. |
| "I'll read the Spec and skip the RFC" | If an RFC exists, read it -- it explains *why*. Standalone specs are fine without one. |
| "I'll look things up as I go" | Ad-hoc context gathering misses the big picture. Resolve the full chain. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec context <id> --json` has been run on the target document
- [ ] `lazyspec show --json` has been run on documents where the body is needed (Spec contracts, RFC design)
- [ ] Existing plans and ADRs have been checked (via `lazyspec status --json` or search)
- [ ] Context chain has been stated back (RFC intent if exists, Spec contracts, prior work)

## Rules

- Always resolve context before starting implementation
- Read the full Spec contracts before writing any code
- Check for existing plans to avoid duplicating work
- Search for types and symbols mentioned in the Spec before creating new ones
