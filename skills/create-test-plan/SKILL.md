---
name: create-test-plan
description: Use when generating a manual end-user test plan across one or more Stories. Gathers RFC and Story context, extracts acceptance criteria, and produces a checklist an end-user or QA tester can execute without reading source code.
---

```
TEST PLANS VERIFY BEHAVIOUR, NOT IMPLEMENTATION
```

A test plan describes what an end-user does and what they should observe. It never references internal code, function names, or test harnesses.

<HARD-GATE>
Do NOT produce automated test code. This skill outputs a manual test plan document.
Do NOT write steps that require reading source code or running a test suite.
Every step must be executable by someone with access to the built application and its CLI/UI only.
After completion: present the test plan to the user for review.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT include implementation details, file paths, or function names in test steps.
- Do NOT skip stories. Every in-scope Story AC must map to at least one test case.
- Do NOT invent acceptance criteria. Test cases derive from documented ACs only.
</NEVER>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Workflow

```d2
Identify scope -> Gather context -> Extract ACs -> Group into test scenarios -> Write test plan -> Validate -> Present to user

Identify scope -> Single story or RFC?

Single story or RFC?.shape: diamond
Single story or RFC? -> List stories under RFC -> Gather context: rfc
Single story or RFC? -> Gather context: story
```

## Steps

1. **Identify scope** with the user. Either:
   - A single Story ID
   - An RFC ID (all Stories implementing it)
   - A list of Story IDs

2. **Gather context:**
   - For an RFC: `lazyspec list story --json` and filter by `implements` relationship to the RFC
   - For each Story: `lazyspec show <story-id> --json` to read ACs
   - For the parent RFC: `lazyspec show <rfc-id> --json` for design intent and user-facing behaviour descriptions
   - Use `lazyspec show -e <id>` if `@ref` directives reference user-facing documentation

3. **Extract acceptance criteria** from every in-scope Story. Build an AC registry: Story ID, AC number, given/when/then text. Every AC must appear in the final plan.

4. **Group into test scenarios.** A scenario is a coherent user workflow that may cover multiple ACs across stories. Group by:
   - User goal or workflow (e.g. "create and publish a document")
   - Precondition similarity (scenarios sharing the same "given" can share setup)
   - Dependency order (scenario B depends on state produced by scenario A)

5. **Write the test plan.** Present partition to user for approval, then create the document:
   - `lazyspec create spec "<title>" --author <name>` (or the document type the user prefers)
   - `lazyspec link <plan-path> related-to <story-path>` for each story covered

   Each test scenario contains:
   - **Title:** short description of the user goal
   - **Preconditions:** what must be true before starting (installed version, existing data, permissions)
   - **Steps:** numbered, imperative instructions using only the application's interface (CLI commands, UI actions)
   - **Expected results:** observable outcomes after each significant step
   - **ACs covered:** which Story ACs this scenario verifies (e.g. STORY-042 AC 1, 3)

6. **Traceability check:** Verify every AC in the registry maps to at least one scenario. Flag any gaps.

7. **Validate:** `lazyspec validate --json`

8. **Present** the test plan to the user. Do NOT begin testing.

## Test Scenario Format

```markdown
### Scenario: <user goal>

**Preconditions:** <setup state>
**Covers:** STORY-NNN AC 1, STORY-NNN AC 3

- <what the user does> — <what they should observe>
- <next action> — <expected result>
```

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "Run `cargo test`" in a step | This is a manual plan. No test harnesses. |
| "Open `src/foo.rs`" in a step | End-users don't read source code. |
| "This AC is covered by unit tests" | Unit tests and manual test plans serve different purposes. |
| "I'll skip that story, it's internal" | If it has user-observable ACs, it needs test cases. |
| "I'll add extra scenarios for edge cases I thought of" | Derive from documented ACs only. Flag gaps to the user instead. |

## Verification

- [ ] Every in-scope Story AC maps to at least one test scenario
- [ ] No step references source code, internal APIs, or test frameworks
- [ ] Each scenario has preconditions, steps, and expected results
- [ ] `lazyspec validate --json` passes
- [ ] Document linked to all covered stories
- [ ] Test plan presented to user for review
