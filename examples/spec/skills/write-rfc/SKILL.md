---
name: write-rfc
description: Use for heavy, cross-cutting, or architecturally significant designs. Creates an RFC document with intent, interface sketches, and identifies the Specs that fall out of it. Most features skip RFCs and go straight to Spec.
---

```
RFCS ARE FOR HEAVY DESIGN WORK
```

RFCs capture design intent for cross-cutting, project-level, or architecturally significant changes. Most features don't need an RFC -- they go straight to Spec. Use an RFC when the work spans multiple areas, involves significant trade-offs, or needs broader alignment.

<HARD-GATE>
Do NOT create Specs until this RFC is written and the user has approved it.
After completion: use the `/create-spec` skill for each contract slice identified.
</HARD-GATE>

> [!IMPORTANT]
> Read `_common.md` in the skills directory for CLI usage, forbidden actions, and subagent tiers.

<NEVER>
- Do NOT create Spec documents from this skill. Finish the RFC, get approval, then use the `/create-spec` skill.
</NEVER>

# Write RFC

## Workflow Position

```d2
lazy -> write-rfc -> create-spec -> resolve-context -> create-plan -> build

write-rfc.style.fill: "#4A9EFF"
write-rfc.style.font-color: "#FFFFFF"
lazy.style.opacity: 0.4
create-spec.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-lazy.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
Understand the problem -> Create RFC -> Write intent and context -> Sketch interfaces -> Identify specs -> Validate -> User approves?

User approves?.shape: diamond
User approves? -> Use /create-spec skill: yes
User approves? -> Revise RFC: no
Revise RFC -> Write intent and context

Use /create-spec skill.shape: double_circle
```

## Preflight

1. Read relevant documents using `lazyspec show --json` before modifying anything
2. Check for existing artifacts using `lazyspec search --json` and `lazyspec list --json`
3. Search for existing RFCs on the topic: `lazyspec search "<topic>" --json`, `lazyspec list rfc --json`
4. Read any related RFCs with `lazyspec show <id> --json`
5. Confirm no existing RFC already covers this design

## Steps

1. **Understand the problem:** Search existing docs with `lazyspec search <topic> --json` to avoid duplicating prior work. Check for superseded RFCs.

2. **Create the RFC:** Run `lazyspec help create` to confirm usage, then: `lazyspec create rfc "<title>" --author <name>`

3. **Write intent:** Describe the problem being solved and why. This is design intent, not implementation detail.

4. **Sketch interfaces:** Use `@draft` syntax for types that don't exist yet:
   ```
   @draft UserProfile { id: string; email: string }
   ```
   Use `@ref` for types that already exist in the codebase:
   ```
   @ref src/types/user.ts#UserProfile
   ```

5. **Identify Specs:** List the contract slices that fall out of this RFC. Each spec should lock down the contracts for one vertical slice -- data models, API surface, validation, error handling, edge cases. An implementer should be able to build from a spec without making design judgment calls.

6. **Emit ADRs:** For significant decisions made during RFC writing, run `lazyspec help create` to confirm usage, then: `lazyspec create adr "<decision>"`. Run `lazyspec help link` to confirm usage, then: `lazyspec link <adr-path> related-to <rfc-path>`.

7. **Validate:** Run `lazyspec validate --json`.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I'll just start coding and document later" | Documentation after = rationalisation. Write the RFC. |
| "This is too small for an RFC" | Maybe it is. RFCs are for heavy/cross-cutting work. Most features just need a Spec. |
| "I already know the design" | If it's not written down, it doesn't exist. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate --json` passes
- [ ] User has explicitly approved the RFC
- [ ] At least one Spec has been identified
- [ ] Any significant decisions have ADRs

## Rules

- RFCs describe intent, not implementation
- An RFC is a design record -- it captures thinking at the time of writing
- Sketch interfaces in prose or TypeScript, not as live code
- Every RFC should identify at least one Spec
