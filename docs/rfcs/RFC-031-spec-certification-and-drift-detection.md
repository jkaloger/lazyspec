---
title: "Spec Certification and Drift Detection"
type: rfc
status: draft
author: "jkaloger"
date: 2026-03-24
tags: [certification, specs, drift-detection, architecture]
related:
  - related-to: "docs/rfcs/RFC-019-inline-type-references-with-ref.md"
  - related-to: "docs/rfcs/RFC-001-my-first-rfc.md"
---

## TL;DR

Specs are collections of acceptance criteria (AC) pinned to code via `@ref` directives. Five signals -- symbol drift, test drift, test failure, AC mutation, scope change -- independently inform whether those AC are still true. When signals converge, a human re-certifies. Blob hashes (`@{blob:hash}`) replace commit SHAs for pinning, surviving squash merges and GC. story.md is a pure AC list; all `@ref` directives live in index.md. Certification is one commit, not two.

## Summary

Lazyspec documents today serve a delivery pipeline: RFCs propose, Stories scope, Iterations execute. This works for shipping code, but there's no mechanism to assert that documentation remains true over time. Architecture documents describe the system, but nothing enforces -- or even detects -- when the codebase drifts away from what they describe.

This RFC introduces two changes:

1. Evolve the `arch` document type into `spec` -- a persistent contract type whose scope is defined by its `@ref` directives and whose behavioural claims are defined by acceptance criteria in `story.md`
2. Add a multi-signal drift detection system and human-driven certification workflow that together answer one question: are the spec's AC still true?

The central thesis: the code is the specification. Specs _describe_ what code does for users, pinned to the implementation and tests that prove it. When the code changes, the spec is stale. Certification means "this description is verified accurate" -- backed by converging signals, not just a human's word.

## Problem

Architecture documents in lazyspec are static. Once written and accepted, they sit in `docs/architecture/` with no connection to reality. A developer can change `SessionManager::create` in ways that contradict ARCH-003's description of the engine, and nothing surfaces this. The document stays "accepted" while the code moves on.

The `@ref` directive system (RFC-019) already creates machine-readable links between documents and code symbols. Today these links are used for rendering -- expanding `@ref src/engine/document.rs#DocMeta` into a code block. But they also represent a spec's _scope_: the set of symbols it makes claims about. This scope is the foundation for drift detection.

## Two-Axis Model

Lazyspec documents serve two distinct concerns that cross-reference each other:

```d2
direction: right

contract: Contract Layer {
  style.fill: "#e8f0fe"

  spec: Spec
  cert: Certification
  refs: "@ref directives"

  spec -> refs: "scope defined by"
  cert -> spec: "asserts conformance of"
}

delivery: Delivery Layer {
  style.fill: "#fce8e6"

  rfc: RFC
  iteration: Iteration

  rfc -> iteration: "broken into"
}

delivery.rfc -> contract.spec: "related-to (designs)"
delivery.iteration -> contract.spec: "implements / affects"
contract.cert -> delivery.iteration: "enabled by"
```

The contract layer is persistent truth: specs describe the system, `@ref` directives pin them to code, AC define behavioural claims, certification records a human conformance assertion in frontmatter. The delivery layer is ephemeral work: RFCs propose changes to specs, iterations execute the work. Specs bridge the two layers -- they own both the contract (AC, `@ref` scope) and the delivery tracking (which iterations implement them).

## Design

### The AC as Contract

The atomic unit of a spec's contract is the acceptance criterion. Each AC is a given/when/then statement that makes a behavioural claim about the system. The spec's `@ref` directives -- both implementation symbols and test functions -- define the scope of code that these claims are about. Together, the AC and the refs form the contract surface.

No single signal can determine whether an AC is true. A passing test doesn't prove the AC is correct (the test might not check what the AC claims). A stable symbol doesn't prove the AC holds (the behaviour might have changed without altering the AST shape). But multiple signals converging provide increasing confidence. The system's job is to collect these signals, present them per-spec, and let a human decide when re-certification is needed.

### Signal Model

> [!WARNING]
> No signal is a verdict. Each is a triage input. They gain meaning through convergence.

The system collects five signal types, all of equal weight, all feeding the same question: "should someone review this spec's AC?"

| Signal       | Source                              | What it means                              | What it doesn't mean                        |
| ------------ | ----------------------------------- | ------------------------------------------ | ------------------------------------------- |
| Symbol drift | `@ref` impl targets in `index.md`   | Implementation code changed since baseline | The AC is violated                          |
| Test drift   | `@ref` test targets in `index.md`   | The test code changed since baseline       | The AC is no longer verified                |
| Test failure | Test runner execution               | A referenced test does not pass            | The spec is wrong (the test might be wrong) |
| AC mutation  | `story.md` content hash             | The behavioural claims changed             | The code needs to change                    |
| Scope change | AC added or removed from `story.md` | The contract surface grew or shrank        | Previous certifications are meaningless     |

Signals compose. A single drifted symbol in a spec with 5 stable refs and passing tests is low urgency -- something changed, but everything else looks fine. Three drifted symbols, a changed test, and a failing test is high urgency -- the ground has shifted under multiple AC simultaneously.

The system deliberately avoids language that implies any single signal = violation or absence of signal = conformance. Drift is a triage signal ("something changed, you should look"), not a verdict ("the spec is broken").

### Spec Requirements

A spec is a certifiable contract. Every spec must have:

1. At least one blob-pinned `@ref` directive in its index targeting implementation symbols, defining the spec's scope in code
2. A `story.md` sub-document with given/when/then acceptance criteria, defining behavioural claims

Test `@ref` directives in `index.md` are strongly recommended but not required for certification. A spec with implementation refs and AC but no test refs is certifiable -- it just lacks the test-failure signal, reducing the system's ability to detect conformance gaps automatically. `lazyspec validate` warns about specs with no test refs.

Certification status is binary at the spec level: a spec is either certified (has `certified_by` and `certified_date` in frontmatter) or uncertified. Binary certification forces the spec owner to consider the whole contract when certifying, rather than cherry-picking easy AC and deferring hard ones.

### Spec Type (evolving from `arch`)

The `arch` document type becomes `spec`. Existing architecture documents (ARCH-001 through ARCH-005) migrate to SPEC-001 through SPEC-005. The directory structure moves from `docs/architecture/` to `docs/specs/`.

A spec is a document that describes how a part of the system works _right now_, verified by the code and tests it references. What distinguishes it from an RFC is persistence and verifiability: an RFC captures design thinking at a point in time, while a spec describes the current steady state and can prove it through deterministic signals. An RFC might produce or modify specs; the spec outlives the RFC.

A spec's scope is defined by its `@ref` directives. A spec that references `src/engine/document.rs#DocMeta` and `src/engine/store.rs#Store::load` is declaring that it makes claims about those symbols. This is not a new mechanism -- `@ref` already exists. The change is in treating `@ref` as a scope declaration, not just a rendering convenience.

A spec's behavioural claims are defined by its `story.md` sub-document. Each criterion is a given/when/then statement that describes observable behaviour tied to the symbols in scope. The AC are the contract; `@ref` directives tell you where to look when something changes.

### Spec Scope

A spec should cover a _coherent behavioural unit_: a set of related functions, types, and files that together implement a recognisable capability. The right size is one where a reader can understand the entire contract in a single sitting, and where drift in any part of the scope is likely relevant to the rest.

Concrete guidance on granularity:

| Too narrow                 | Right size                                                   | Too broad         |
| -------------------------- | ------------------------------------------------------------ | ----------------- |
| A single struct definition | Session lifecycle (create, refresh, expire, revoke)          | "The engine"      |
| One CLI flag               | Document validation pipeline (rules, diagnostics, reporting) | "The CLI"         |
| A config field             | @ref expansion (parsing, resolution, caching, rendering)     | "All of lazyspec" |

The heuristic: if you certify a spec and one of its symbols changes, should you care? If the answer is "probably not, it's unrelated" -- the spec is too broad and should be split. If you find yourself certifying three specs every time you touch one function -- the specs are too narrow and should be merged.

Specs can cover different kinds of things:

- Functional behaviour: how a capability works end-to-end. These are the most common specs. Example: "how document validation works" covering the rule engine, issue types, and diagnostic output.
- Data contracts: the shape of key types and their invariants. Example: "the document model" covering DocMeta, frontmatter parsing, and relationship types.
- Integration boundaries: how components connect. Example: "CLI-to-engine interface" covering the command routing and store initialisation.
- Configuration and schema: structure of config files, templates, or data formats. For non-code files (JSON, TOML, YAML), `@ref` without a symbol references the entire file: `@ref .lazyspec.toml`. Drift detection for whole-file refs compares the file's blob hash at the pinned baseline vs HEAD.

Specs should not try to cover user-facing workflows end-to-end. A workflow like "user searches documents" might touch three specs: the search algorithm, the CLI output formatting, and the TUI rendering. Each spec stands on its own.

### Spec Structure

A spec is a directory containing two files, following the same nested document pattern used by architecture documents today:

```
docs/specs/SPEC-001-validation/
├── index.md    ← the spec itself (type: spec, @ref directives, prose)
└── story.md    ← acceptance criteria (type: spec, given/when/then only)
```

#### `index.md` -- scope and prose

The index contains the spec's prose description, `@ref` directives (both implementation and test targets), and any diagrams or tables that describe the system's behaviour. This is the stable part of the spec -- it changes when the system's architecture changes.

Certification metadata (`certified_by`, `certified_date`, `story_hash`) lives in the index frontmatter.

All `@ref` directives live in `index.md`, including references to test functions. This keeps the machine-readable scope in one place and keeps `story.md` purely human-readable.

#### `story.md` -- acceptance criteria

The story contains given/when/then criteria that define behavioural claims. Nothing else. No `@ref` directives, no frontmatter beyond the standard lazyspec header.

```markdown
---
title: "Session Lifecycle"
type: spec
status: draft
author: jkaloger
date: 2026-03-24
---

### AC1: Expired sessions are rejected

Given a session older than the configured TTL
When the engine evaluates it
Then it returns a SessionExpired error

### AC2: Refresh extends TTL

Given a valid, non-expired session
When refresh is called
Then the session's expiry is extended by TTL

### AC3: Revoked sessions are immediate

Given an active session
When the session is explicitly revoked
Then subsequent requests with that session token are rejected
```

story.md is the human-readable contract. It answers "what should be true?" without coupling to specific test functions or code locations. The correlation between AC and signals (which tests exercise which claims, which symbols are relevant to which AC) is a human judgment made during certification, not a structural pairing enforced by the tooling.

The story.md is a sub-document of the spec (both have `type: spec`), not a standalone document type. It inherits the spec's relationships and appears as part of the spec in `lazyspec status`.

#### story.md lifecycle

story.md changes are tracked through content hashing, not through `@ref` directives. At certification time, `lazyspec certify` computes a hash of story.md's content and stores it as `story_hash` in the index frontmatter. Subsequent drift detection compares the current story.md content against this hash.

This catches three scenarios the `@ref`-only model misses:

- AC added: story.md hash changes, spec is flagged as uncertified (the contract grew)
- AC removed: story.md hash changes, spec is flagged as uncertified (a claim was withdrawn)
- AC prose changed: story.md hash changes, spec is flagged as uncertified (the claim shifted)

A `status: draft` spec suppresses the story_hash check, allowing AC to be written and revised freely during development. The hash check activates when the spec reaches `status: accepted` and has been certified at least once.

#### Iterations

Iterations link to specs via `implements` relationships. They are not listed in the spec's markdown -- the relationship graph is the source of truth. `lazyspec status` reconstructs which iterations implement a spec from the relationship data.

### Scope Constraints

`@ref` directives define a spec's scope, but scope is only useful if it's honest. Two failure modes undermine the system: under-scoping (referencing fewer symbols to avoid drift noise) and over-scoping (referencing entire files or broad modules, creating constant drift).

To keep scope meaningful:

`lazyspec validate` enforces a maximum ref count per spec. The default ceiling is 15 symbols. A spec referencing more than this is likely too broad and should be split. The limit is configurable in `.lazyspec.toml` for projects with different needs. This is a warning, not a hard error, but it creates friction against sprawl.

Specs must reference symbols they actually describe. A ref that appears in the document but is never discussed in the surrounding prose is a coverage-padding signal. The `/write-spec` skill checks for this: every `@ref` should appear within a paragraph that makes a claim about the referenced symbol's behaviour, shape, or invariants. Orphan refs (present but undiscussed) trigger a validation warning.

Cross-module refs require justification. A spec referencing symbols from more than 3 distinct modules is likely describing an interaction pattern rather than a coherent unit. This isn't prohibited, but `lazyspec validate` surfaces it as an advisory. If the spec genuinely covers a cross-cutting concern (like error propagation across layers), that's fine. If it's accumulated scope creep, it should be split.

Whole-file refs (`@ref path@{blob:hash}` without a symbol) are intentionally coarse-grained and should be used sparingly. They're appropriate for config files, schemas, and templates where symbol-level extraction doesn't apply. For source files, prefer symbol-level refs. A whole-file ref on a 500-line Rust module will drift on every change to any symbol in that file, generating noise.

> [!NOTE]
> These constraints address the gaming incentive directly: under-scoping is caught by the "orphan ref" check (you can't claim coverage you don't describe), and over-scoping is caught by the ref count limit and cross-module advisory.

### Ref Variants and Blob Pinning

The `@ref` directive supports several forms. Pinned refs use git blob hashes rather than commit SHAs, which makes them resilient to squash merges, rebases, and garbage collection.

| Form                           | Targets                     | Drift detection                 | Example                                             |
| ------------------------------ | --------------------------- | ------------------------------- | --------------------------------------------------- |
| `@ref path#symbol@{blob:hash}` | A named symbol, pinned      | Blob hash comparison            | `@ref src/engine/document.rs#DocMeta@{blob:a1b2c3}` |
| `@ref path@{blob:hash}`        | An entire file, pinned      | Blob hash comparison            | `@ref .lazyspec.toml@{blob:d4e5f6}`                 |
| `@ref path#symbol`             | A symbol at HEAD (unpinned) | No baseline, no drift detection | `@ref src/engine/document.rs#DocMeta`               |
| `@ref path`                    | A file at HEAD (unpinned)   | No baseline, no drift detection | `@ref .lazyspec.toml`                               |

#### Why blob hashes, not commit SHAs

The original design used commit SHAs (`@ref path#symbol@abc123f`) and resolved the old state via `git show abc123f:path`. This breaks under squash merge: the commit is orphaned from main's history, and `git gc` eventually prunes it (immediately on shallow clones, within 30-90 days otherwise). Once pruned, `git show` fails and drift detection breaks permanently for that spec.

Blob hashes avoid this entirely. A blob hash identifies a specific piece of _content_, not a specific _commit_. When a file is squash-merged to main, the blob persists in the repository because the content exists in main's tree. The original branch commit can be garbage collected without affecting drift detection.

#### How blob pinning works

For symbol-level refs, the blob hash is computed from the tree-sitter-extracted symbol content:

1. Extract the symbol from the source file using tree-sitter
2. Hash the extracted bytes: `echo -n "<symbol content>" | git hash-object --stdin`
3. Store as `@ref path#symbol@{blob:a1b2c3}`

For whole-file refs, the blob hash is the file's git object ID:

1. `git hash-object path/to/file` (computable from the working tree, before commit)
2. Store as `@ref path@{blob:d4e5f6}`

Drift detection compares the stored blob hash against the current state:

1. Extract the symbol (or read the file) at HEAD
2. Hash the result
3. If hashes differ: DRIFTED. If the symbol cannot be found: ORPHANED. If identical: CURRENT.

To show _what_ changed (not just _that_ it changed), `git diff <old-blob> <new-blob>` produces a content diff between the two blob objects. This works because both blobs exist in the object store -- the old one persists as long as the content appears anywhere in reachable history.

> [!WARNING]
> Blob hash stability depends on tree-sitter extraction consistency. If a tree-sitter grammar update changes how symbols are parsed (e.g. attribute node boundaries shift), all stored hashes for that language become stale, producing phantom drift across every spec. Mitigate this by pinning tree-sitter grammar versions in `Cargo.toml` and treating grammar bumps as certification-invalidating events that require a re-pinning pass (`lazyspec pin --all`).

Whole-file blob hashing is sensitive to _any_ change in the file, including unrelated ones. If a spec pins `@ref .lazyspec.toml@{blob:abc123}` and a developer adds an unrelated config key, the blob hash changes and the spec reports drift. Mitigations: keep config files that specs pin small and focused. Structured file targeting (e.g. JSONPath or jq-style selectors) is out of scope for this RFC but remains a natural future extension.

### Coverage (Advisory)

Coverage is _not_ a percentage to chase. Reporting "60% of symbols are covered by specs" creates a vanity metric that incentivises shallow specs written to pad a number. Internal helpers, test utilities, and glue code don't need specs. Measuring coverage as a ratio treats all symbols as equally important, which they aren't.

Instead, `lazyspec status` surfaces qualitative coverage signals:

- Public symbols with no spec reference (exported functions, public structs/enums/traits that no spec's `@ref` touches)
- Modules with zero spec coverage where user-facing behaviour lives
- Non-code files (config, schemas, templates) that no spec references

These appear as advisories in `lazyspec status --json`, not as a standalone `coverage` command:

```
$ lazyspec status --json
{
  ...
  "coverage_advisories": [
    { "file": "src/engine/session.rs", "public_symbols": 8, "covered": 0, "note": "no spec references any symbol in this module" },
    { "file": ".lazyspec.toml", "covered": false, "note": "config file not referenced by any spec" }
  ]
}
```

The `/write-spec` skill can use these advisories to suggest where new specs would be most valuable. But the system never reports a coverage percentage or treats low coverage as a problem to solve.

### Drift Detection

Every spec with blob-pinned `@ref` directives gets automatic drift detection. The mechanism collects signals from all five types and presents them per-spec.

#### Symbol and test drift

For each pinned ref in `index.md`:

1. Extract the symbol at HEAD using tree-sitter (or read the whole file for file-level refs)
2. Hash the result
3. Compare against the stored blob hash

Three outcomes:

- CURRENT: blob hash is identical
- DRIFTED: symbol exists but content differs
- ORPHANED: symbol cannot be found at HEAD (deleted or renamed)

DRIFTED refs are a triage signal. ORPHANED refs are a harder failure: the spec references something that no longer exists. `lazyspec validate` treats orphaned refs as errors, since the spec's scope is broken.

```d2
direction: down

refs: "Collect pinned @ref targets" {
  ref1: "document.rs#DocMeta@{blob:a1b2}"
  ref2: "store.rs#Store::load@{blob:c3d4}"
}

hash_stored: "Read stored blob hash" {
  style.fill: "#e8f0fe"
  note: "from @{blob:...} in @ref directive"
}

hash_head: "Compute blob hash at HEAD" {
  style.fill: "#e8f0fe"
  note: "tree-sitter extract → git hash-object"
}

compare: "Compare hashes" {
  shape: diamond
}

current: "CURRENT" {
  style.fill: "#e6f4ea"
}

drifted: "DRIFTED" {
  style.fill: "#fce8e6"
}

orphaned: "ORPHANED" {
  style.fill: "#fff3e0"
  note: "symbol missing at HEAD"
}

refs -> hash_stored
refs -> hash_head
hash_stored -> compare
hash_head -> compare
compare -> current: "identical"
compare -> drifted: "differs"
compare -> orphaned: "not found at HEAD"
```

This reuses the existing tree-sitter symbol extraction from the `@ref` expansion pipeline. The only new work is hashing at two states and comparing.

#### AC mutation and scope change

Drift detection compares the current story.md content hash against the `story_hash` stored in frontmatter at certification time. Any change -- added AC, removed AC, modified prose -- produces a mismatch, flagging the spec as needing re-certification.

This is deliberately coarse-grained. The system doesn't track which specific AC changed; it signals that the contract surface moved. The human reviewer determines what changed and whether it matters.

#### Test failure

When a test runner is configured in `.lazyspec.toml`, `lazyspec drift` can optionally execute the spec's referenced test functions and report pass/fail as an additional signal. This is off by default for `lazyspec status` (which should be fast) but available via `lazyspec drift --run-tests <spec-id>` and always active during `lazyspec certify`.

#### Drift suppression during active development

When a developer is mid-implementation on a cross-cutting feature, `lazyspec status` shows drift in every spec they've touched. This is accurate but unhelpful: they already know they're making changes and haven't finished yet.

`lazyspec status` uses iteration relationships to distinguish expected from unexpected drift. When an iteration with `status: draft` (in-progress work) has `implements` or `affects` relationships to specs, drift in those specs is tagged as _expected_ in the output:

```
$ lazyspec status
SPEC-003: Validation Pipeline
  drift: 1 drifted (expected — ITERATION-091 in progress)

SPEC-004: Configuration Schema
  drift: 1 drifted (expected — ITERATION-091 in progress)

SPEC-007: Search Algorithm
  drift: 1 drifted ← UNEXPECTED
```

The `(expected)` annotation doesn't hide the drift; it contextualises it. `lazyspec status --unexpected-only` filters to show only unexpected drift. When the iteration is marked `accepted`, the expected-drift tagging stops and all drift becomes actionable again.

#### Signal convergence in output

`lazyspec status` presents signals per-spec, showing which signal types fired:

```
SPEC-002: Data Model
  status: accepted
  certified: 2026-03-20 (jkaloger)
  signals:
    symbol drift: 1 of 3 impl refs changed
      src/engine/document.rs#DocMeta@{blob:a1b2} — DRIFTED
      src/engine/store.rs#Store::load@{blob:c3d4} — CURRENT
      src/engine/session.rs#SessionManager::create@{blob:e5f6} — CURRENT
    test drift: 0 of 2 test refs changed
    story.md: UNCHANGED
  urgency: low (single symbol drift, tests stable)
```

A higher-urgency example:

```
SPEC-005: Search Algorithm
  status: accepted
  certified: 2026-02-15 (jkaloger) — STALE
  signals:
    symbol drift: 3 of 4 impl refs changed
    test drift: 1 of 2 test refs changed
    test failure: tests/search_test.rs#test_ranked_results — FAILING
    story.md: CHANGED (AC added since certification)
  urgency: high (multiple signals converging)
```

### Explicit Certification

Drift detection tells you signals fired. Certification is the human response: "I have reviewed the signals, confirmed the AC are still accurate, and the system agrees -- tests pass and symbols resolve."

Certification is stored in the spec's `index.md` frontmatter:

```yaml
---
title: "Data Model"
type: spec
status: accepted
certified_by: jkaloger
certified_date: 2026-03-20
story_hash: "sha256:aabb1122..."
---
```

The `story_hash` field captures the content hash of `story.md` at certification time. If `story.md` changes after certification, the hash mismatch signals that the contract surface has moved and re-certification is needed.

#### The `lazyspec certify` command

1. Resolves all `@ref` targets in the spec at HEAD to verify they're resolvable
2. Pins any unpinned refs to HEAD by computing their blob hashes and writing `@{blob:hash}` into the directives
3. If a test runner is configured, runs the test functions referenced by `@ref` test targets and verifies they pass
4. If all checks pass, writes `certified_by`, `certified_date`, and `story_hash` to the spec's frontmatter
5. If any test fails, reports the failure and refuses to certify

The command mutates the file on disk but does not commit. The developer reviews the diff and commits as part of their normal workflow.

#### Single-commit workflow

Blob hashes are computable from the working tree _before_ a commit is made. This eliminates the two-commit problem that exists with commit-SHA-based pinning.

The workflow:

1. Developer writes code, tests, and updates the spec (prose, AC, `@ref` directives)
2. Developer runs `lazyspec certify SPEC-004`
3. The CLI extracts each symbol from the working tree, computes blob hashes, writes `@{blob:hash}` pins and certification frontmatter into the spec file
4. Developer stages everything (`git add`) and commits in a single commit

This works because `git hash-object <file>` produces the same hash whether called on the working tree file or on the committed blob -- they're the same content. The hash computed in step 3 matches the blob stored by git in step 4.

> [!NOTE]
> `lazyspec pin <spec-id>` is available as a standalone command for developers who want to pin refs without certifying. This is useful when updating spec prose without asserting conformance, or when preparing a spec for future certification.

#### Squash merge

Blob pinning makes squash merge a non-event for drift detection. The blob hash identifies file _content_, not a commit. When a feature branch is squash-merged to main, the file content lands in the squash commit's tree -- the blob object persists in main's history. Drift detection compares blob hashes, not commit ancestry. `git diff <old-blob> <new-blob>` works because both blobs are reachable.

The only caveat: if the squash merge modifies files after the certification point (e.g. resolving conflicts that change a referenced symbol), the blob hash at HEAD won't match the certified baseline. This is correct behaviour -- the system detects that the squashed content diverges from what was certified, and the developer re-certifies on main.

For projects using squash merge, the practical workflow is: certify on the branch (satisfying PR review), merge, accept any post-squash drift, and re-certify periodically (e.g. before releases) or on main after merge.

#### Certification and test execution

The test execution step is what gives certification teeth. A developer cannot certify a spec whose tests fail, regardless of their confidence that the AC hold. This eliminates the rubber-stamping failure mode.

Test execution requires a configured test runner in `.lazyspec.toml`. For Rust projects this defaults to `cargo test`. The `certify` command extracts test function names from the `@ref` test targets and passes them as filters to the runner. If no test runner is configured, `certify` falls back to symbol resolution only (drift detection without behavioural verification) and emits a warning that certification is partial.

> [!NOTE]
> Local test execution is a convenience, not a hard gate for every environment. The definitive verification should happen in CI, where the environment is controlled. `lazyspec certify --skip-tests` allows certification without running tests locally (emitting a warning), while `lazyspec validate --strict` in CI runs the full test suite as a non-bypassable check.

When a certified spec drifts, re-certification is needed:

```
SPEC-002: Data Model
  status: accepted
  certified: 2026-03-20 (jkaloger) — STALE
  signals:
    symbol drift: 1 of 3 impl refs changed
      src/engine/document.rs#DocMeta — DRIFTED (since certification)
```

Re-certification: resolve symbols at HEAD, re-pin drifted refs, verify tests, update frontmatter. Certification history is available through `git log` on the spec file, since each certification touches the frontmatter.

### Relationship Model Changes

The `implements` relationship becomes the primary link between delivery and contract layers. Iterations `implements` Spec, meaning the iteration is work that moves code toward conformance with that spec.

One new relationship type:

| Relationship            | Meaning                                                                          | Example                             |
| ----------------------- | -------------------------------------------------------------------------------- | ----------------------------------- |
| `implements` (existing) | Delivery chain + iteration-to-spec                                               | ITERATION-090 `implements` SPEC-003 |
| `affects` (new)         | An iteration changes code covered by a spec, without necessarily implementing it | ITERATION-091 `affects` SPEC-002    |

The distinction: `implements` means "this iteration is intentionally working toward this spec's conformance." `affects` means "this iteration touched symbols that fall within this spec's scope" -- it may be incidental. Both are useful for surfacing which specs need attention after iterations land.

RFCs relate to specs through the existing `related-to` type. An RFC proposes design thinking that may lead to new specs or modifications to existing ones, but the relationship is advisory, not mechanical.

### Audits, Iterations, and Specs

Audits, iterations, and specs form a feedback cycle that drives continuous conformance:

```d2
direction: right

spec: Spec {
  style.fill: "#e8f0fe"
  note: "describes the contract"
}

audit: Audit {
  style.fill: "#fff3e0"
  note: "reviews conformance"
}

iteration: Iteration {
  style.fill: "#fce8e6"
  note: "implements fixes"
}

cert: Certification {
  style.fill: "#e6f4ea"
  note: "asserts conformance"
}

spec -> audit: "audited against"
audit -> iteration: "findings drive"
iteration -> spec: "implements"
iteration -> cert: "enables re-certification"
cert -> spec: "records conformance in"
```

Every transition in this cycle is human-initiated. The system surfaces signals; humans decide when to act. Nothing auto-creates documents.

#### Transition: Drift -> Audit

`lazyspec status` surfaces signal counts per spec, sorted by certification status (certified specs first). A developer or agent sees converging signals on SPEC-002 and decides whether to investigate. The decision to audit is theirs -- the system does not auto-create audit documents when signals fire.

The `/audit-cert` skill is the entry point. A developer invokes it explicitly, either for a specific spec or broadly (the skill reads `lazyspec status` and presents all specs with signals for selection). Without invocation, signals sit in `lazyspec status` output as passive information.

#### Transition: Audit -> Iteration

An audit documents findings: which specs have active signals, whether the drift is a conformance gap (code diverged from spec) or an intentional change (spec needs updating). The audit does not auto-create iterations.

The developer reads the audit findings and decides what to do. For conformance gaps, they create iterations that fix the code to match the spec. For intentional changes, they create iterations that update the spec prose and `@ref` pins to match the new code.

#### Transition: Iteration -> Re-certification

Once an iteration lands (code merged, spec updated if needed), the spec is eligible for re-certification. The system does not auto-certify. The `/build` skill surfaces affected specs after completion and suggests running `/certify-spec`. The developer reviews the signals, confirms the spec accurately describes the current code, and runs `lazyspec certify`.

@ref src/engine/document.rs#RelationType

### Required Relationships Rework

The current workflow enforces a strict pipeline through Stories and RFCs. With specs as the contract layer owning both AC and delivery tracking, this gatekeeping needs revision. If a spec already describes the contract, iterations can implement it directly.

The revised model recognises three entry points depending on the size and nature of the work:

| Work size                | Entry point                               | Pipeline                             |
| ------------------------ | ----------------------------------------- | ------------------------------------ |
| Routine spec work        | Iteration directly implements spec        | Iteration -> certify                 |
| Multi-iteration delivery | Spec's delivery section groups iterations | Spec -> Iterations -> certify        |
| Cross-cutting design     | RFC proposes new/changed specs            | RFC -> Spec -> Iterations -> certify |

RFCs are no longer required for all new features. They're required when the design is non-trivial or crosses multiple specs. A spec that already exists and just needs implementation work can go straight to iterations.

Validation rules change accordingly:

- Iterations _must_ link to a Spec via `implements`. An orphan iteration with no spec is a validation error.
- Specs have no required parent. They can originate from an RFC (`related-to`) or be created standalone.

### Workflow and Document Set

The complete document set after this RFC:

| Type        | Purpose                                     | Persistence                          | Certifiable                |
| ----------- | ------------------------------------------- | ------------------------------------ | -------------------------- |
| `spec`      | Behavioural contract (index.md + story.md)  | Persistent, evolves over time        | Yes (requires `@ref` + AC) |
| `rfc`       | Design rationale, proposes changes          | Point-in-time, may become superseded | No                         |
| `iteration` | Task breakdown, unit of implementation work | Ephemeral, accepted when done        | No                         |
| `audit`     | Conformance review, documents findings      | Point-in-time snapshot               | No                         |
| `adr`       | Architecture decision record                | Point-in-time, records a decision    | No                         |

> [!NOTE]
> The standalone `story` document type is superseded. Stories become `story.md` sub-documents within spec directories. Existing Story documents should be migrated: their AC move into the relevant spec's `story.md`, and their iterations re-link to the spec via `implements`.

The overall workflow with certification:

```d2
direction: down

ideation: "1. Ideation" {
  style.fill: "#e8f0fe"
  rfc: "RFC (if cross-cutting)"
  spec: "Write or update Spec"
  rfc -> spec: "designs"
}

delivery: "2. Delivery" {
  style.fill: "#fce8e6"
  iteration: "Iterations (linked to spec)"
}

review: "3. Review" {
  style.fill: "#fff3e0"
  code_review: "Code review"
  ac_review: "AC compliance"
}

certification: "4. Certification" {
  style.fill: "#e6f4ea"
  audit: "Audit (optional)"
  certify: "lazyspec certify"
  audit -> certify: "findings resolved"
}

drift: "5. Signal Collection" {
  style.fill: "#f3e8ff"
  monitor: "lazyspec status"
  alert: "Converging signals surfaced"
}

ideation -> delivery: "spec ready"
delivery -> review: "code complete"
review -> certification: "review passed"
certification -> drift: "certified"
drift -> ideation: "signals trigger re-work"
```

### Ref Index

Several operations need to answer the reverse question: "given a file or symbol that changed, which specs reference it?" Scanning every spec document and parsing its `@ref` directives on every `lazyspec status` invocation is O(specs \* refs-per-spec). On a project with hundreds of specs, this becomes a bottleneck.

The ref index is a pre-computed mapping from ref targets to the specs that reference them:

```json
{
  "src/engine/document.rs#DocMeta": ["SPEC-002", "SPEC-005"],
  "src/engine/store.rs#Store::load": ["SPEC-002"],
  ".lazyspec.toml": ["SPEC-004"],
  "src/cli/commands.rs#run_certify": ["SPEC-003"]
}
```

The index is built by scanning all spec documents under `docs/specs/` and extracting their `@ref` targets (stripping the `@{blob:...}` suffix). It is stored at `.lazyspec/cache/ref-index.json` and rebuilt when any spec file's mtime is newer than the index file's mtime. On a cold cache or after spec changes, the rebuild is a single pass over the spec directory. On a warm cache, the index is read directly.

Commands that use the index:

- `lazyspec status` uses it to generate coverage advisories and to sort drift reports
- `lazyspec drift` uses it when invoked without a spec ID to find all specs affected by a given commit range
- The `/build` skill uses it to answer "which specs does this iteration affect?"

The index is a cache, not a source of truth. If the index is missing or corrupt, commands fall back to the full scan. The index is gitignored (it's derived data).

### Agent Skills

Certification introduces three new skills and modifies several existing ones.

#### New: `/write-spec`

Creates or updates spec documents. The skill guides agents through:

1. Identifying the scope of the spec (which modules, which symbols)
2. Writing the `index.md` with prose descriptions, `@ref` directives for both implementation symbols and test functions, and diagrams
3. Writing the `story.md` with given/when/then acceptance criteria only (no `@ref` directives)
4. Validating that all refs in `index.md` resolve and symbols exist

The skill should encourage specs that are _scoped tightly enough to certify_. Agents writing specs should prefer `@ref` over prose when describing code structure. Prose describes intent and invariants; `@ref` pins the actual implementation.

#### New: `/certify-spec`

Runs the certification workflow for one or more specs:

1. Run `lazyspec drift <spec-id>` to check current signal state
2. If signals are active, present findings to the user -- the spec may need updating before certification
3. If the user confirms, run `lazyspec certify <spec-id>` which resolves refs, pins blobs, optionally runs tests, and writes frontmatter on success
4. If tests fail, present the failures and block certification
5. If tests pass, present the frontmatter diff to the user for review
6. The developer commits the change as part of their normal workflow
7. Run `lazyspec validate` to confirm no new issues

#### New: `/audit-cert` (certification audit)

A specialised audit skill that reviews spec conformance. Always human-invoked.

1. Run `lazyspec status` to identify all specs with active signals
2. Present specs to the user for selection
3. For each selected spec, assess whether the signals represent a conformance gap or an intentional change
4. Document findings as an audit
5. Propose next steps (iterations to fix code or update specs)

The skill proposes next steps but does not auto-create iterations or trigger re-certification.

#### Modified: `/plan-work`

The plan-work skill needs to recognise specs as a first-class entry point:

- When searching for existing artifacts, also check `lazyspec drift` output for stale specs related to the topic
- Classification expands: "spec conformance work" joins "new feature," "bug fix," etc. as a work type
- Entry point logic changes: if a spec exists for the area being worked on, the iteration implements the spec directly
- The skill should surface stale specs when the user describes work in a spec's domain, even if the user didn't mention certification

#### Modified: `/create-audit`

The create-audit skill gains certification-aware criteria:

- New audit type: "spec conformance" alongside existing general-purpose audits
- For spec conformance audits, criteria are derived from the spec's `@ref` targets and prose claims
- The skill should run `lazyspec drift` as part of its preflight to seed the audit with known signals

#### Modified: `/build`

After build completion, the build skill should:

- Check which specs are affected by the iteration's changes
- Surface any specs with new signals to the user
- Suggest running `/certify-spec` for affected specs
- This is a prompt, not an automatic action

#### Modified: `/review-iteration`

The review skill's AC compliance check should also verify spec conformance:

- If the iteration `implements` a spec, check that the spec's `@ref` targets still resolve after the iteration's changes
- If the iteration introduced drift in a spec it `affects`, flag this in the review
- This doesn't block the review -- drift is expected during active development. But it surfaces the certification debt early.

### Failure Modes

Systems that don't design for failure fail in ways that are hard to recover from.

_Nobody certifies._ Drift detection still functions -- it compares against the in-file `@ref` blob pins (the authoring baseline). The system degrades gracefully: you lose the "human verified AC conformance" signal but retain the "something changed" signal. Uncertified specs with AC are still useful -- the AC document intent even without formal certification.

_Everything drifts._ When many specs show signals simultaneously (e.g. after a large refactor), triage by signal convergence: specs with multiple signal types firing get attention first. If drift is widespread enough that the signal becomes noise, the correct response is to update the specs to reflect the new reality (if the refactor was intentional) or to revert (if it wasn't).

_Specs are wrong._ A certified spec that incorrectly describes the code is the worst failure mode. Test refs reduce this risk: if the tests accurately exercise the AC claims, a passing test suite means the behavioural claims hold. The remaining gap is between what the AC _say_ and what the tests _check_ -- a test that doesn't actually verify its AC is a spec bug. The system mitigates this by making certification a deliberate act with a visible audit trail.

_Tests don't match AC._ The system verifies that referenced tests _pass_, not that they _verify the specific claim_ in the AC. This is the system's largest trust assumption. Mitigations: (1) PR review, where the reviewer follows the `@ref` to the test and confirms the test exercises the claim; (2) the `/audit-cert` skill, which assesses AC-test alignment; (3) test drift detection, which flags when a test function changes. The gap between "test passes" and "test verifies the AC" is fundamentally a code review problem, and the system makes it _auditable_ rather than _enforceable_.

_Spec rot (abandoned specs)._ A spec that nobody updates and nobody certifies will accumulate drift until it's meaningless. `lazyspec validate` surfaces specs where all refs have drifted as "fully drifted" with an advisory to either update or archive.

_Scope gaming._ The scope constraints (ref count limits, orphan ref detection, cross-module advisories) create friction against both under-scoping and over-scoping. They don't prevent gaming entirely, but they make the lazy path (honest scoping) easier than the gaming path.

_Certification ceremony overhead._ Not every modification to a symbol covered by a spec requires immediate spec updates. Drift between certification cycles is acceptable and expected. The right cadence depends on the project: some teams certify on every PR, others certify before releases. The system supports both by making signals visible without making them blocking.

_Tree-sitter version drift._ A tree-sitter grammar update can change how symbols are extracted, invalidating all blob hashes for that language. This produces phantom drift (every spec reports DRIFTED despite no source changes). Pin grammar versions in `Cargo.toml` and treat bumps as re-pinning events. `lazyspec pin --all` re-computes all blob hashes after a grammar update.

### CLI Changes

New commands:

- `lazyspec certify <spec-id>` -- resolve refs, pin blobs, run referenced tests, certify at HEAD if all pass (writes `certified_by`, `certified_date`, `story_hash` to frontmatter)
- `lazyspec certify <spec-id> --skip-tests` -- certify without running tests (emits warning)
- `lazyspec drift <spec-id>` -- show detailed signal report for a spec
- `lazyspec drift <spec-id> --run-tests` -- include test execution in the signal report
- `lazyspec pin <spec-id>` -- pin all unpinned `@ref` directives to their current blob hash without certifying
- `lazyspec pin --all` -- re-pin all refs across all specs (useful after tree-sitter grammar updates)

Changes to existing commands:

- `lazyspec status` gains a signal summary section for all specs, sorted by certification status, with expected/unexpected drift tagging
- `lazyspec status --unexpected-only` filters to show only unexpected signals
- `lazyspec status` gains a coverage advisories section
- `lazyspec drift --file <path>` shows which specs reference symbols in the given file (reverse index lookup)
- `lazyspec validate` gains errors for: specs missing `@ref` directives in `index.md`, specs missing `story.md`, orphaned refs. Gains warnings for: specs with no test refs, specs exceeding the ref count limit, cross-module ref spread, fully-drifted specs, story_hash mismatch on accepted specs
- `lazyspec validate --strict` (for CI) additionally runs referenced tests and fails on test failures or story_hash mismatch
- `lazyspec create` accepts `spec` as a document type (replacing `arch`)

### Migration

Two migrations are needed:

#### Arch to Spec

1. `type: arch` in frontmatter becomes `type: spec`
2. Files move from `docs/architecture/` to `docs/specs/` (directory structure is preserved)
3. Document IDs change from ARCH-XXX to SPEC-XXX
4. Relationships pointing to arch documents update their targets
5. Each migrated spec needs a `story.md` added to its directory (initially empty, to be filled in during the first certification pass)

#### Story to Spec

Existing Story documents are superseded. For each Story:

1. Identify the spec the Story's work relates to (create a new spec if none exists)
2. Migrate the Story's acceptance criteria into the spec's `story.md` (stripping any `@ref` directives -- story.md is AC only)
3. Move any test `@ref` directives from the Story into the spec's `index.md`
4. Re-link the Story's iterations to the spec via `implements`
5. Mark the Story as `status: superseded`

Stories don't need to be deleted immediately. They can remain as historical artifacts with `status: superseded`. `lazyspec validate` should warn about iterations still linked to superseded Stories, prompting re-linking to specs.

A `lazyspec migrate` command handles this. Alternatively, the initial migration can be performed manually given the small number of existing documents.

## Stories

1. Spec type and migration -- evolve `arch` to `spec`, migrate existing documents, update CLI and validation
2. Spec nested structure -- implement index.md + story.md directory layout with story.md as pure AC, migrate existing Story AC into spec story.md files, re-link iterations to specs
3. Blob-pinned drift detection -- implement `@{blob:hash}` ref format, compute blob hashes from tree-sitter extraction, compare against HEAD, surface in `lazyspec status`
4. Signal model and output -- collect all five signal types per spec, present with convergence-based urgency, expected/unexpected tagging
5. Explicit certification -- `lazyspec certify` writes certification metadata and story_hash to frontmatter, single-commit workflow via blob hashing
6. Scope constraints and coverage advisories -- ref count limits, orphan ref detection, cross-module advisories, qualitative coverage signals
7. Relationship model extensions -- add `affects` relationship type, `implements` for iteration-to-spec, supersede Story document type, update validation rules
8. Agent skills for certification -- `/write-spec`, `/certify-spec`, `/audit-cert` skills, plus modifications to `/plan-work`, `/create-audit`, `/build`, `/review-iteration`
