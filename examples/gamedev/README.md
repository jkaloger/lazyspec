# Solo agentic game-dev loop

## The premise

Code is the source of truth for what the game _does_. Docs never mirror current behaviour -- they hold either **judgment** (durable, rarely changes) or **work** (disposable, consumed then archived). Two invariants make the loop safe:

1. **No orphan work.** Every `delta` must `serves` an _accepted_ `pillar`, or the engine refuses it. Scope creep is structurally impossible.
2. **Fun is discovered, not specified.** No `delta` reaches `done` without a human playtest. Failing the playtest routes work _back to code_, not forward.

## The six document types

| Axis       | Type                       | Holds                                              | Who writes        |
| ---------- | -------------------------- | -------------------------------------------------- | ----------------- |
| Durable    | `convention` / `principle` | coding standards the build agent obeys             | you (assisted)    |
| Durable    | `pillars`                  | 3-5 experience goals; the scope spine              | you (assisted)    |
| Durable    | `adr`                      | architecture rationale (the _why_ code can't hold) | you (assisted)    |
| Disposable | `prototype`                | throwaway to de-risk fun or tech; verdict only     | agent (generated) |
| Disposable | `delta`                    | shippable change; feature or fix                   | agent (generated) |

## Bootstrap (once per project)

```
lazyspec init
lazyspec create convention   # coding principles; accept it
lazyspec create pillars       # the 3-5 experience goals; ACCEPT it
```

Until a pillar is `accepted`, nothing can be built. That is deliberate -- commit to what the game _is_ before building toward it.

## The inner loop (per feature)

```
                         ┌───────────────────────────────────────────┐
                         │  idea / bug / "would this be fun?"        │
                         └───────────────────┬───────────────────────┘
                                             │
                    unknown risk?            │            known enough?
              ┌──────────────────────────────┴───────────────────────┐
              ▼                                                        ▼
      ┌───────────────┐                                        ┌───────────────┐
      │  prototype    │  build throwaway, PLAY it,             │     delta     │
      │  (fun|tech)   │  record verdict ──── informs ────────▶ │  serves pillar│
      └───────┬───────┘                                        │  tag: system  │
              │ concluded                                      └───────┬───────┘
              └── kills a bad idea cheaply                             │ draft
                                                                       ▼
                                                                    ready
                                                                       │  ← build agent picks up here
                                                            ┌──────────┘
                                                            ▼
                                                        building ──▶ playtest
                                                            ▲            │
                                              not fun       │            │  YOU play it
                                            (back to code)  └────────────┤
                                                                         ├──▶ done      (feels right)
                                                          feel wrong     └──▶ draft     (redesign)
```

### Step by step

1. **De-risk first (optional).** Unsure a mechanic is fun, or an architecture holds up? `lazyspec create prototype` (tag `risk = fun` or `tech`). Agent builds the minimum, _you play it or read it_, verdict goes in the body, `advance` to `concluded`. It `informs` the delta or adr it de-risked. Throw the code away.

2. **Record structural decisions.** Reaching for ECS, a fixed tick, an event bus? `lazyspec create adr`, drive `draft -> review -> accepted`. It `governs` the deltas that rely on it, so you can find affected work before you ever change your mind.

3. **Write the delta.** `lazyspec create delta`. It must `serves` a pillar (the engine enforces this) and carry a `system` tag (combat, movement, ...). Big feature? Break it into child deltas linked `part-of` the parent. The agent authors the body from pillars + convention + linked adrs; you review and `advance` to `ready`.

4. **Build unattended.** The build agent takes `ready -> building -> playtest`: writes code against the delta's acceptance criteria and your `convention`, runs tests, then **stops**. It cannot certify fun.

5. **Playtest (human gate).** You play it.
   - Feels right -> `advance` to `done`.
   - Correct but not fun -> back to `building` (tweak the code/tuning).
   - Idea is wrong -> back to `draft` (redesign, maybe spawn a `prototype`).

6. **Repeat.** Bugs are just deltas that fix rather than add -- same lifecycle, usually a lighter playtest.

## Command / skill map

| Intent                      | Command / skill                                                      |
| --------------------------- | -------------------------------------------------------------------- |
| Start any work              | `/lazy` (dispatches the right verb from your position)               |
| New doc                     | `lazyspec create <type>` / `/scaffold` / `/co-write` / `/generate`   |
| Move a doc forward          | `lazyspec advance <id>` / `/advance`                                 |
| Run a delta's build         | `/execute` (the build agent)                                         |
| Batch-run many ready deltas | `/orchestrate`                                                       |
| Critique before advancing   | `/review`                                                            |
| See a delta's lineage       | `lazyspec context <id>` (walks pillar ← delta ← children)            |
| What's touching combat?     | `lazyspec list delta --json` then filter `system`                    |
| Health check                | `lazyspec validate` (flags any delta not serving an accepted pillar) |

## What the two invariants buy you

- `lazyspec validate` failing on `deltas-serve-pillars` is your scope-creep alarm: a delta exists that no experience goal justifies. Delete it or find its pillar.
- The `playtest -> building` edge is the only place the loop runs backward on purpose. If your board shows deltas piling up in `playtest`, you have built faster than you have played -- the signal to stop building and start playing.

## Caveats

- `require_parent_status = "accepted"` gating a delta on its `pillars` singleton is configured but not runtime-verified in this example. Smoke-test it before relying on the block.
- Art, audio, and level _production_ aren't modeled here -- only agent-buildable code work. Track asset production wherever you make it.
- Release phases (prototype / vertical slice / alpha / beta / gold) are intentionally unmodeled. If you want a home for exit criteria, add a `milestone` type and a `targets` relation.
