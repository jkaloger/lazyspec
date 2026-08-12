# Solo agentic game-dev loop

## The premise

Code is the source of truth for what the game _does_. Docs never mirror current behaviour -- they hold either **judgment** (durable, rarely changes) or **work** (disposable, consumed then archived). Two invariants make the loop safe:

1. **No orphan work.** Every `delta` must `serves` an _accepted_ `pillar`, and every `iteration` must `implements` a delta or prototype, or the engine refuses them. Scope creep is structurally impossible.
2. **Fun is discovered, not specified.** No `delta` reaches `done` without a human playtest. Failing the playtest routes work _back to code_, not forward.

## The two-level split

Work has two levels, and they judge different things:

- A **`delta`** (or **`prototype`**) says **WHAT** and **WHY**. Fun is judged here, by you, at the playtest gate.
- Its **`iteration`s** say **HOW**: detailed, agent-executable build plans. Correctness is judged here -- an iteration is done when its tests pass. Usually several iterations per parent.

The build agent lives entirely at the iteration level. The delta never gets built directly; it gets _planned into iterations_, the agent runs them, and only then does the delta reach playtest.

## The seven document types

| Axis       | Type                       | Holds                                              | Who writes        |
| ---------- | -------------------------- | -------------------------------------------------- | ----------------- |
| Durable    | `convention` / `principle` | coding standards the build agent obeys             | you (assisted)    |
| Durable    | `pillars`                  | 3-5 experience goals; the scope spine              | you (assisted)    |
| Durable    | `adr`                      | architecture rationale (the _why_ code can't hold) | you (assisted)    |
| Disposable | `prototype`                | throwaway to de-risk fun or tech; verdict only     | agent (generated) |
| Disposable | `delta`                    | shippable change (WHAT/WHY); feature or fix        | agent (generated) |
| Disposable | `iteration`                | agent-executable build plan (HOW)                  | agent (generated) |

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
              │  idea / bug / "would this be fun?"          │
              └───────────────────┬─────────────────────────┘
                                  │
         unknown risk?            │            known enough?
   ┌──────────────────────────────┴───────────────────────┐
   ▼                                                        ▼
┌───────────────┐                                    ┌───────────────┐
│  prototype    │  de-risk: build throwaway,         │     delta     │
│  (fun|tech)   │  PLAY it, record verdict           │  serves pillar│
└───────┬───────┘  ───── informs ─────────────────▶  │  tag: system  │
        │ concluded                                   └───────┬───────┘
        └── kills a bad idea cheaply                          │ draft
                                                              ▼
                                                            ready ── broken into iterations
                                                              │
                    ┌─────────────────────────────────────────┘
                    ▼   each iteration `implements` the delta
             ┌─────────────┐   ┌─────────────┐   ┌─────────────┐
             │ iteration ▸ │   │ iteration ▸ │   │ iteration ▸ │   ← build agent
             │ ready→build │   │ ready→build │   │ ready→build │      runs these
             │ →done       │   │ →done       │   │ →done       │      unattended
             └──────┬──────┘   └──────┬──────┘   └──────┬──────┘
                    └─────────────────┴──── all done ───┘
                                      ▼
                              delta: in-progress ──▶ playtest
                                          ▲             │
                            not fun       │             │  YOU play it
                          (more iters)    └─────────────┤
                                                        ├──▶ done   (feels right)
                                        feel wrong      └──▶ draft  (redesign)
```

### Step by step

1. **De-risk first (optional).** Unsure a mechanic is fun, or an architecture holds up? `lazyspec create prototype` (tag `risk = fun` or `tech`). Plan it into a throwaway iteration or two, agent builds the minimum, _you play it or read it_, verdict goes in the prototype body, `advance` to `concluded`. It `informs` the delta or adr it de-risked. Throw the code away.

2. **Record structural decisions.** Reaching for ECS, a fixed tick, an event bus? `lazyspec create adr`, drive `draft -> review -> accepted`. It `governs` the deltas that rely on it, so you can find affected work before you ever change your mind.

3. **Write the delta.** `lazyspec create delta`. It must `serves` a pillar (the engine enforces this) and carry a `system` tag (combat, movement, ...). The agent authors the WHAT/WHY body from pillars + convention + linked adrs; you review and `advance` to `ready`.

4. **Plan iterations.** Break the delta into `iteration`s, each a self-contained build plan that `implements` the delta -- one agent session's worth of work. The agent drafts them; you sanity-check the breakdown and `advance` each to `ready`.

5. **Build unattended.** The build agent takes each iteration `ready -> building -> done`: writes code against the iteration's plan and your `convention`, runs tests, marks it `done` when green (or `blocked` if it hits an unknown). Run them one at a time or batch with `/orchestrate`. When every iteration is `done`, the delta moves `in-progress -> playtest`.

6. **Playtest (human gate).** You play the delta.
   - Feels right -> `advance` to `done`.
   - Correct but not fun -> back to `in-progress`, write more iterations (tune, tweak).
   - Idea is wrong -> back to `draft` (redesign, maybe spawn a `prototype`).

7. **Repeat.** Bugs are just deltas that fix rather than add -- same lifecycle, usually one iteration and a lighter playtest.

## Command / skill map

| Intent                        | Command / skill                                                      |
| ----------------------------- | -------------------------------------------------------------------- |
| Start any work                | `/lazy` (dispatches the right verb from your position)               |
| New doc                       | `lazyspec create <type>` / `/scaffold` / `/co-write` / `/generate`   |
| Move a doc forward            | `lazyspec update <id> --status <next>` / `/advance`                  |
| Run one iteration's build     | `/execute` (the build agent)                                         |
| Batch-run many ready iters    | `/orchestrate`                                                       |
| Critique before advancing     | `/review`                                                            |
| See an iteration's lineage    | `lazyspec context <id>` (walks pillar ← delta ← iteration)           |
| What's touching combat?       | `lazyspec list delta --json` then filter `system`                    |
| Health check                  | `lazyspec validate` (flags orphan deltas and orphan iterations)      |

## What the invariants buy you

- `lazyspec validate` failing on `deltas-serve-pillars` is your scope-creep alarm: a delta exists that no experience goal justifies. Failing on `iterations-implement-work` means an iteration floated free of any delta/prototype -- work with no reason. Delete it or link it.
- The `playtest -> in-progress` edge is the only place the loop runs backward on purpose. If your board shows deltas piling up in `playtest`, you have built faster than you have played -- the signal to stop building and start playing.
- The delta/iteration split keeps two questions separate: "is this correct" (iteration tests) and "is this fun" (delta playtest). An iteration can be perfectly green and still feed a delta you bounce.

## Caveats

- `require_parent_status = "accepted"` gating a delta on its `pillars` singleton is configured but not runtime-verified in this example. Smoke-test it before relying on the block.
- `iterations-implement-work` uses relation-existence, which checks that _any_ relation exists -- normally the `implements` link. An iteration carrying only a `blocks` link would technically satisfy it; in practice iterations always implement their parent.
- Art, audio, and level _production_ aren't modeled here -- only agent-buildable code work. Track asset production wherever you make it.
- Release phases (prototype / vertical slice / alpha / beta / gold) are intentionally unmodeled. If you want a home for exit criteria, add a `milestone` type and a `targets` relation.
