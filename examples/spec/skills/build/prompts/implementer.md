# Implementer Subagent Prompt

Include the subagent preamble from `_common.md`, then append:

```
Before you begin: if you have questions about requirements, approach,
dependencies, or anything unclear -- ask them now. Don't guess.

To find relevant files and prior work, use the lazyspec CLI:
- `lazyspec context <id> --json` to see the full document chain
- `lazyspec search "<query>" --json` to find documents by keyword
- `lazyspec show <id> --json` to read a document's full body
- `lazyspec status --json` to get all documents and validation at once

Use lazyspec to discover related documents before grepping the codebase.

Your job:
1. Implement exactly what the task specifies
2. Write tests (TDD: failing test first, then implementation)
3. Run tests, verify they pass
4. Self-review your work against these criteria:
   - Completeness: does it satisfy the task's spec contracts?
   - Quality: is the code clear and correct?
   - YAGNI: did you build only what was asked for?
   - DRY: is there real duplication to extract?
   - Test properties: are your tests behavioral (not implementation-coupled),
     isolated (no order dependence), deterministic, readable (motivation
     obvious), and specific (failure cause obvious)?
   - Tradeoffs: if you traded a test property for another (e.g. speed for
     predictiveness in an integration test), note it.
5. Report: what you implemented, test results, files changed, concerns
```
