---
title: "Example Skill Evidence-Based Review"
type: audit
status: draft
author: "agent"
date: 2026-03-11
tags: [skills, examples, evidence-based]
related: []
---

## Scope

Audit of the example skills under `examples/spec/` against the Evidence-Based Skill Development guide (Superpowers-derived). Audit type: spec compliance / design review.

Covers all 8 skills, 5 prompt templates, shared `_common.md`, and 4 document templates.

## Criteria

Evidence-Based Skill Development guide, covering:

1. Description-as-pure-trigger (Section 2.3)
2. Skill type classification and structural signatures (Section 3)
3. Token budgets (Section 2.4)
4. Iron Law pattern (Section 8.1)
5. Rationalization tables (Section 8.2)
6. Red flags lists (Section 8.3)
7. Flowcharts for loops (Section 8.4)
8. Two-stage separation (Section 8.5)
9. Separate prompts for separate roles (Section 8.7)
10. Suggestion vs directive language (Section 9.6)
11. Cross-referencing with requirement markers (Section 4.3)
12. Supporting file separation (Section 4.4)

## Findings

### Finding 1: No flowcharts for process skills with loops

**Severity:** high
**Location:** `examples/spec/skills/build/SKILL.md`, `examples/spec/skills/review-plan/SKILL.md`, `examples/spec/skills/lazy/SKILL.md`
**Description:** The guide (Section 8.4) states process skills need Graphviz DOT flowcharts specifically at loop points where agents might stop prematurely. The `build` skill has a per-task loop with contract-fail branching (re-dispatch implementer on failure) that is described only in prose. The `review-plan` skill's two-stage gate with failure-loopback is also prose-only. Prose loops like "review again if needed" get compressed to "review and fix" by agents.
**Recommendation:** Add DOT flowcharts to `build` (per-task dispatch/review loop), `review-plan` (two-stage gate with loopback), and `lazy` (routing decision tree). Make backtracking arrows explicit.

### Finding 2: No rationalization tables

**Severity:** high
**Location:** All skill files, particularly `examples/spec/skills/build/SKILL.md`, `examples/spec/skills/review-plan/SKILL.md`, `examples/spec/skills/create-plan/SKILL.md`
**Description:** The guide's most distinctive pattern (Section 8.2) maps specific excuses to specific counters. None of the skills have one. The `build` skill has red flags, but red flags catch symptoms while rationalization tables preempt the reasoning that leads to violations. Likely rationalizations: "This task is simple enough to implement myself", "The reviewer already checked, no need for final review", "I'll batch these two small tasks into one subagent", "The plan is clear enough without exact file paths."
**Recommendation:** Add rationalization tables to `build`, `review-plan`, and `create-plan`. Source rationalizations from actual agent sessions (RED phase testing) or from the anti-patterns already documented in `_common.md`.

### Finding 3: `_common.md` mixes discipline, reference, and process concerns

**Severity:** medium
**Location:** `examples/spec/skills/_common.md`
**Description:** The shared file contains CLI usage rules (reference), forbidden actions (discipline), subagent tier guidance (reference), status promotion workflow (process), and an anti-patterns table (discipline). The guide recommends different structural signatures for different skill types (Section 3). Mixing them means discipline elements don't get the enforcement language they need, and reference elements add noise around the directives. The forbidden actions section uses `<NEVER>` tags which is good, but it sits between reference material, diluting its urgency.
**Recommendation:** Split into at least two files: hard rules (forbidden actions, anti-patterns) with stronger enforcement language, and reference material (CLI usage, subagent tiers, status promotion).

### Finding 4: Token budgets not managed

**Severity:** medium
**Location:** `examples/spec/skills/create-plan/SKILL.md`, `examples/spec/skills/build/SKILL.md`
**Description:** The guide sets targets (Section 2.4): always-loaded < 150 words, frequent < 200 words, situational < 500 words. Several skills exceed 500 words. `create-plan` inlines a full test quality properties list (11 properties with descriptions) that costs tokens on every plan creation. The guide asks that every paragraph justify its token cost: "Does this change agent behavior, or does it just make me feel more thorough?"
**Recommendation:** Move the test quality properties list to a supporting file. Audit word counts across all skills. Inline only what directly changes agent behavior.

### Finding 5: `lazy` skill uses suggestion language for routing

**Severity:** medium
**Location:** `examples/spec/skills/lazy/SKILL.md`
**Description:** The entry-point skill uses softer language than the others. Routing logic is described in prose rather than explicit decision tables. The guide (Section 9.6) warns that "generally," "when possible," and descriptive prose are escape hatches agents exploit under pressure. Since `lazy` determines the entire downstream workflow, an agent rationalizing a shortcut here cascades through everything.
**Recommendation:** Replace prose routing with an explicit decision table or flowchart. Make routing rules absolute: "If no Spec exists for a new feature, invoke `/create-spec`. No exceptions."

### Finding 6: `create-spec` authoring principles read as suggestions

**Severity:** medium
**Location:** `examples/spec/skills/create-spec/SKILL.md`
**Description:** Principles like "ACs are core, appear early" and "No open questions in accepted specs" use descriptive language rather than directives. The guide (Section 2.2) distinguishes suggestions ("Consider writing tests first when feasible") from directives ("Write code before test? Delete it. Start over."). Spec quality cascades into plan and build quality, so weak enforcement here has outsized downstream impact.
**Recommendation:** Rewrite as directives: "ACs MUST appear before Data Models", "An accepted spec with unresolved questions is a rejected spec", "Specs over 100 lines MUST be split."

### Finding 7: Cross-references lack requirement markers

**Severity:** low
**Location:** All skill files, prompt templates
**Description:** The guide (Section 4.3) specifies explicit requirement markers: `**REQUIRED SUB-SKILL:** Use superpowers:test-driven-development`. Skills reference each other by name but don't distinguish required from optional references. Prompt templates reference behaviors but don't cross-reference the skills those behaviors come from. An agent may not realize it needs to load a referenced skill.
**Recommendation:** Add `**REQUIRED SUB-SKILL:**` or `**OPTIONAL REFERENCE:**` markers to all cross-references. In prompt templates, explicitly name the skill whose conventions apply.

### Finding 8: No evidence of RED-phase pressure testing

**Severity:** low
**Location:** All skills (process gap, not a file-level issue)
**Description:** The guide's core thesis (Section 5.1): "If you didn't watch an agent fail without the skill, you don't know what the skill needs to prevent." The anti-patterns table in `_common.md` hints at observed failures, but skills don't show signs of hardening against specific rationalizations discovered through the RED-GREEN-REFACTOR cycle. This is a process gap rather than a content gap, but it means the skills may have blind spots the author hasn't encountered yet.
**Recommendation:** Run pressure scenarios (Section 5.2) against `build` and `review-plan` specifically. Combine 3+ pressure types. Document rationalizations verbatim and feed them back into the skills.

### Finding 9: Description-as-pure-trigger is well-implemented

**Severity:** info
**Location:** All skill files
**Description:** Skills follow the "Use when [situation]" pattern without summarizing workflow in descriptions. This matches the guide's strongest anti-pattern warning (Section 2.3, 9.3). No action needed.
**Recommendation:** Maintain this pattern as skills evolve.

### Finding 10: Two-stage separation and separate prompts are well-implemented

**Severity:** info
**Location:** `examples/spec/skills/review-plan/SKILL.md`, `examples/spec/skills/build/prompts/`
**Description:** `review-plan` enforces contract compliance before code quality with a hard gate. `build` uses distinct `implementer.md`, `reviewer.md`, and `final-reviewer.md` prompts with role-specific framing. Both match the guide's recommended patterns (Sections 8.5, 8.7).
**Recommendation:** Maintain this pattern. Consider adding the same two-stage separation to `create-audit` if it ever gains automated remediation.

### Finding 11: Hard gates used effectively as Iron Laws

**Severity:** info
**Location:** `examples/spec/skills/build/SKILL.md`, `examples/spec/skills/review-plan/SKILL.md`, `examples/spec/skills/create-plan/SKILL.md`
**Description:** Several skills use hard gates that match the Iron Law pattern (Section 8.1): absolute, testable, early in the document. "Do NOT implement without a Plan", "Do NOT approve without running tests in this session", "Do NOT write test code or production code."
**Recommendation:** Maintain. Consider adding Iron Laws to `lazy` and `create-spec` where routing and spec quality enforcement are currently soft.

## Summary

The example skills are a well-structured workflow system. The document hierarchy, subagent separation, two-stage review, hard gates, and description-as-trigger pattern all align with the guide's recommendations.

The main gaps are in resilience under pressure. The skills lack the defensive patterns (rationalization tables, flowcharts for loops, directive language) that the guide identifies as necessary for agents to comply when they're tempted to cut corners. The highest-impact improvements would be adding flowcharts to `build` and `review-plan` (where agents skip loop iterations), and adding rationalization tables to the three skills where agents face the most pressure.

| Severity | Count |
|----------|-------|
| Critical | 0 |
| High | 2 |
| Medium | 4 |
| Low | 2 |
| Info | 3 |
