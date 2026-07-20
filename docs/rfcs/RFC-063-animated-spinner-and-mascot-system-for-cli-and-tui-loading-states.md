---
title: "Animated spinner and mascot system for CLI and TUI loading states"
type: rfc
status: draft
author: "Jack Kaloger"
date: 2026-07-20
tags: [tui, cli, ux]
related: []
---
<!-- intent: propose a design and the decisions it forces, before code -->

## Summary

Add an extensible spinner framework at crate root (`src/spinners.rs`) plus a first spinner — an **abstract face** (a boxed two-eyes-and-mouth character) — driving every loading state across CLI and TUI. Same frame source feeds both layers; each maps colour to its own terminal type. Inspired by Astro `cli-kit`'s Houston, retargeted to lazyspec's doc-tool identity. No mascot name; it is "the face".

## Motivation

- Loading states today are invisible or static. CLI discards engine progress entirely (no-op `on_progress` closures, `main.rs:194,207`); TUI shows plain status text. `indicatif` is declared but unused (`Cargo.toml:45`).
- Network round-trips block with no feedback: reservation git push, `gh` issue fetch, ClickUp reqwest, remote doc create.
- A recognisable mascot gives lazyspec identity and makes waits legible. Houston proves the pattern works in a CLI.
- Now: the interactive init wizard (RFC-062 line of work) already adds greetings/colour; a mascot lands naturally beside it.

## Goals

- One extensible framework: adding a new spinner = implementing one trait, no changes to call sites.
- The face renders 4 states — idle, loading, success, error — in a full-box form and a compact one-line form.
- Cover all loading surfaces: TUI top-right sync/push indicator, TUI create overlay, CLI long-ops (create/fetch/prune), CLI init-wizard greeting.
- Themed accent colour; dim/bold for state; green success, red error. Respect terminal theme.
- `--json` and non-TTY emit no animation (dictum 2). Machine output unchanged.
- Frame logic pure and unit-testable; no ratatui/crossterm in the logic.

## Non-goals

- No full event-expression map (creating vs validating vs thinking). Core states only; richer expressions deferred.
- No gradient-shimmer colour sweep. Themed accent only.
- No new async runtime for the CLI. Reuse blocking model + a ticker thread.
- Not replacing the status-bar component system (RFC-022); the face plugs into existing zones.

## Design

### Spinner framework (`src/spinners.rs`, crate root)

Layer-agnostic pure module. Neither CLI nor TUI depends on the other; both depend on this. Not under `engine` (engine = doc logic only).

- `enum SpinnerState { Idle, Loading, Success, Error }`
- `enum FrameColour { Accent, Dim, Success, Error }` — semantic, no terminal type. Each layer maps to its own (`ratatui::Style` / ANSI).
- `struct Frame { lines: Vec<String>, colour: FrameColour }` — full-box = multi-line; compact = single line.
- `trait Spinner { fn compact(&self, state: SpinnerState, idx: u64) -> Frame; fn full(&self, state: SpinnerState, idx: u64) -> Frame; }`
- `idx` = monotonic frame counter; each spinner mods it into its own cycle length. Caller owns the clock.
- Registry: `fn spinner(name: &str) -> &'static dyn Spinner` (or lookup map), so future spinners register by name. The face is the default.

Extensibility test: a new spinner is a new type implementing `Spinner`, added to the registry — zero call-site edits.

### The face — frame catalogue

Full box. Inner width is a fixed 7 columns (`╭───────╮`); every glyph is single-width so all frames align exactly. Layout is `[left-eye] [mouth] [right-eye]`.

**idle** — slow blink, 2 frames, ~600ms each:
```
╭───────╮   ╭───────╮
│ ● ▪ ● │   │ - ▪ - │
╰───────╯   ╰───────╯
  [1]         [2]
```

**loading** — eye spin, 4 frames, ~120ms each:
```
╭───────╮   ╭───────╮   ╭───────╮   ╭───────╮
│ ◐ ○ ◐ │   │ ◓ ○ ◓ │   │ ◑ ○ ◑ │   │ ◒ ○ ◒ │
╰───────╯   ╰───────╯   ╰───────╯   ╰───────╯
  [1]         [2]         [3]         [4]
```

**success** — 1 held frame:
```
╭───────╮
│ ◠ ◡ ◠ │
╰───────╯
```

**error** — 1 held frame:
```
╭───────╮
│ × ▂ × │
╰───────╯
```

Compact one-line form (TUI header, CLI inline). Same states, single cluster:

| state    | frames                               |
|----------|--------------------------------------|
| idle     | `[·‿·]`                              |
| loading  | `[◐‿◐]` `[◓‿◓]` `[◑‿◑]` `[◒‿◒]`       |
| success  | `[◠‿◠]`                              |
| error    | `[×_×]`                              |

ASCII fallback set (mirrors Houston `useAscii()`), used when the terminal lacks Unicode: corners `+`, walls `-`/`|`, eyes `o`/`O`/`-`, mouths `o`/`_`/`-`, loading eyes cycle `|`/`/`/`-`/`\`.

### TUI wiring

- Clock: existing ~16ms redraw loop (`event_loop.rs:752-799`); reuse `loop_count` (`:751`, currently unused) as `idx`. No new timer.
- Top-right header (`views.rs:150-180`): replace/augment `right_spans`. Show Loading while `gh_push_in_flight` or poll `refresh_in_flight` active (surface that flag onto `App`); Success briefly after `last_sync`; else Idle.
- Create overlay (`overlays.rs:131-136`): gate the full-box face on `form.loading`; drive from `ReservationProgress` → `SpinnerState`.

### CLI wiring

- Wire `indicatif` (already a dep). `ProgressBar` with `enable_steady_tick` + custom `tick_strings` = the face's compact frames — gives ticker thread + TTY detection + clean teardown for free. Blocking create means `on_progress` only fires on state change, so the steady tick supplies motion.
- Hooks: create closures (`main.rs:194,207`), fetch (`cli/fetch.rs`), prune (`cli/reservations.rs:109-192`). Map progress enums → `SpinnerState`; finish green ✔ / red ✗.
- Greeting: hand-rolled full-box face "say" for the init wizard (Houston `say` equivalent) — talking face (eyes/mouth cycle per word), then resting happy face.
- All CLI spinners guard on `stdout.is_terminal() && !json`.

## Interfaces

- `@draft src/spinners.rs`: `SpinnerState`, `FrameColour`, `Frame`, `trait Spinner`, `struct FaceSpinner`, `fn spinner(name) -> &dyn Spinner`.
- TUI: extend `App` with `refresh_in_flight: bool` (mirrored from event loop); `views.rs` maps `FrameColour` → `ratatui::Style`.
- CLI: helper `fn op_spinner(label, json, tty) -> Option<ProgressBar>` mapping `FrameColour` → ANSI; used by create/fetch/prune.
- No changes to `--json` output shape.

## Decisions (ADRs to emit)

- **ADR: spinner logic lives at crate root, not engine** — cosmetic/layer-agnostic; keeps engine = markdown logic (principle 1, 3).
- **ADR: reuse `indicatif` for CLI, hand-roll greeting box** — indicatif can't render the multi-line face box; use it for the ticker where it fits, hand-roll where it doesn't.
- **ADR: caller-owned frame clock** — pure module takes `idx`; TUI feeds render-loop counter, CLI feeds steady-tick. No timer in the logic.

## Stories

1. **Spinner framework + the face** — `src/spinners.rs`, trait, registry, face frames, unit tests. Foundation; blocks the rest.
2. **TUI loading mascot** — header indicator + create overlay, animation off render loop. Depends on 1.
3. **CLI operation spinners** — indicatif wiring for create/fetch/prune, TTY/json guards. Depends on 1.
4. **CLI greeting face** — init-wizard "say" talking face. Depends on 1; pairs with STORY-229 wizard polish.

## Risks and tradeoffs

- **Unicode glyphs** (`◐ ◓ ◑ ◒ ◠ ◡ ▪ ×`) may not render on all terminals → provide ASCII fallback set (mirror Houston `useAscii()`), pick via env/terminal capability.
- **Redraw cost**: header now animates every 16ms even when idle. Mitigate: only animate in Loading; Idle/Success/Error are static frames.
- **indicatif teardown** interleaving with normal stdout can smear output — draw to stderr, clear on finish.
- **Colour on light themes**: use terminal-default accent + dim/bold, avoid hardcoded hex, so both light and dark themes stay legible.
- Extensible registry is indirection for one concrete spinner today — justified by the explicit goal of pluggable future spinners (accepts principle 6 tension for a stated reason).
