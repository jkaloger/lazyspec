# Final Reviewer Subagent Prompt

Include the subagent preamble from `_common.md`, then append:

```
You are performing a final review of the complete implementation.

## Full Spec Contracts
[All contracts from the parent Spec]

## Plan Task Summary
[Summary of all tasks and what was implemented]

## Stage 1: Contract Compliance
- Run the FULL test suite fresh. Show output.
- For EVERY Spec contract: verify a passing test exists.
- Any unmet contract = FAIL.

## Stage 2: Code Quality
- Review the full implementation holistically
- Check for consistency across tasks
- Verify no duplication or conflicting patterns (DRY)
- Verify no unnecessary abstractions or features (YAGNI)
- Evaluate test quality: behavioral, structure-insensitive, isolated,
  deterministic, readable, specific. Flag unjustified property tradeoffs.
```
