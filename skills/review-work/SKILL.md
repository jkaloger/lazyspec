---
name: review-work
description: Use when critiquing landed code against the document that specified it -- one unit's diff, or a batch's combined diff -- before it is committed or its document is closed.
---

```
CONFORMANCE, CONVENTION, THEN QUALITY
```

Review-work critiques **code that exists** against the acceptance criteria of the document that asked for it. Its sibling /review critiques **documents** against their intent. If you are reading a diff, you are here.

<HARD-GATE>
Run the three stages in order. Do NOT report a quality finding while a conformance or convention finding is open -- the fix will change the code you were about to comment on.
Do NOT approve without evidence you gathered in this session: the diff, the documents, and the gate result you were handed.
Do NOT re-run the gate command. Your caller ran it and hands you the result. If they handed you none, say so and treat the gate as unknown -- do not run it yourself.
Do NOT edit code. You return findings; a separate pass fixes them.
</HARD-GATE>

<NEVER>
- Do NOT edit a document you haven't read. `lazyspec show <id> -e --json` first.
- Do NOT invent acceptance criteria. Every conformance finding cites an AC that is written down.
- Do NOT emit a convention finding without naming the principle or dictum it violates.
</NEVER>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`.

## Depth

Your caller states one. If they state none, assume **comprehensive**.

| | **blocking-only** | **comprehensive** |
|---|---|---|
| When | After one unit's build, before its commit | Once over a batch's whole combined diff |
| Emits | Stage 1 and Stage 2 findings only | All three stages: also nits, naming, duplication across units, dead code, scaffolding left behind, drifted seams, missing tests |
| Stage 3 | Not run | Run, if stages 1 and 2 are clear |
| Dropped findings | Style, naming, structure -- the comprehensive pass catches them | Nothing is dropped |

Neither depth is capped. Emit every finding you have, ranked most severe first. Budgeting *which findings get fixed* is your caller's decision, not yours -- do not truncate to make their job smaller.

## Preflight

1. `lazyspec show <id> -e --json` for the delivery document, and one per ancestor carrying acceptance criteria. These are the bar.
2. `lazyspec context --json` -- the chain, so conformance is judged against the right intent.
3. `lazyspec convention --json` -- the project's principles and dictums. This is Stage 2's bar.
4. The diff. Your caller names the range; read it with `git diff`.

## Stage 1 -- Acceptance conformance

Every acceptance criterion on the delivery document and its ancestors: met, or not. For each unmet one, name the AC and what is missing. A criterion with no test covering it is unmet, however plainly the code appears to do it.

Also flag work the documents did not ask for. Scope creep is a conformance failure in the other direction.

## Stage 2 -- Convention conformance

Read the diff against `lazyspec convention --json`. Every finding **names the principle or dictum it violates, verbatim**, then points at the file and line that violates it. A finding you cannot attribute to a written principle is not a convention finding -- it is Stage 3 quality, and it waits.

Where principles conflict, the convention's own tiebreaker applies; say which one you applied.

Convention findings block. This stage exists because conventions were previously handed to builders and checked by nobody.

## Stage 3 -- Quality

Only when Stages 1 and 2 are clear. Correctness, clarity, YAGNI, DRY, security. Test properties: behavioural, structure-insensitive, isolated, deterministic, readable, specific. Flag unjustified tradeoffs.

## Verdicts

- **GREEN** -- no Stage 1 or Stage 2 findings. Stage 3 notes may still be attached.
- **RED** -- findings a fix pass can clear.
- **STOP** -- the work is not what the document asked for, and fixing it is not a patch job. Use STOP when an AC was not attempted at all, when the gate result you were handed is failing for reasons inside this scope, or when the diff pursues a different design from the one the document specifies. STOP means the caller halts the run and reports; it is not a louder RED.

## Report contract

Return exactly these lines, under 300 words total:

```
VERDICT: GREEN | RED | STOP
UNMET: one line per unmet acceptance criterion, "<AC> — <what is missing>" (omit when none)
CONVENTION: one line per violation, "<principle or dictum, quoted> — <file:line> — <what violates it>" (omit when none)
QUALITY: one line per finding, ranked (omit when not run or none)
NOTED: up to 3 observations worth carrying forward
```

Your evidence gathering and reasoning stay in your own session. The verdict lines are what the caller acts on.
