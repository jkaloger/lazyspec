---
title: "Web view design system: Neo Swiss industrial information design"
type: rfc
status: draft
author: "jkaloger"
date: 2026-06-30
tags: []
related: []
---

<!-- intent: propose a design and the decisions it forces, before code -->

## Summary

A design system for the lazyspec web view, expressed as named design tokens and component specifications rather than implementation. The language is Neo Swiss (International Typographic Style: rigorous asymmetric grid, neo-grotesque type, flush-left ragged-right, objective tone) fused with Industrial Information Design (hairline rules over cards, tabular data, signal-not-decoration color, wayfinding clarity) and Micro Typography (tabular numerals, tracking discipline, a baseline grid, controlled measure). The system targets the existing semantic class hooks already emitted by the Askama templates and ships as static CSS custom properties with no build step, preserving the single-binary, no-Node architecture (ADR-001).

## Motivation

The web view (`src/web/`) serves three surfaces (document, list, graph) plus HTMX fragments (search, filtered list) with zero styling. The templates already carry a disciplined set of semantic classes, so the gap is a token system and a stylesheet, not markup surgery.

Priority problems:

1. No visual hierarchy. Frontmatter, body prose, and navigation render as an undifferentiated wall. A reader cannot scan a document's identity (id, type, status, lineage) at a glance, which is the primary job of a structured-doc tool.
2. No information-density discipline. The graph tree and list are inherently dense (id, type, status, relations per row). Without a tabular, mono-aligned treatment they degrade into ragged text.
3. No theming contract. Dark and light are both expected for a developer-facing reading surface; there is no token layer to make that parity cheap.
4. Status and type are semantic axes (lifecycle states, ten document types) that need consistent, legible encoding across every surface, currently rendered as bare text.

Why now: the web feature is being built out (search, document, graph routes landed). Establishing tokens before more surfaces accrete prevents per-template ad-hoc styling.

## Goals

- A token layer (color, type, space, rule, motion) as CSS custom properties, themeable light/dark from one definition, testable by inspecting computed values.
- A type system: one neo-grotesque for prose and UI, one monospace for all machine identifiers (id, type, status, date, relations), with a defined modular scale and micro-typographic rules (tabular numerals, tracking, measure, baseline rhythm).
- A monochrome ink-on-surface base with a single locked brand accent, plus a separate, documented categorical legend for the lifecycle-status axis that encodes state label-first and color-redundant.
- Component specs mapped 1:1 onto the existing class hooks for document, list, search, and graph surfaces, using hairline rules and negative space instead of cards or shadows.
- Restrained motion: HTMX swap transitions and focus affordance only, fully collapsing under `prefers-reduced-motion`.
- Zero build step. The system is plain CSS plus self-hosted fonts, servable by the existing binary.

## Non-goals

- No implementation. This RFC defines tokens and specs; CSS and font embedding fall out into stories.
- No template/markup changes. The system styles existing class hooks; if a hook is missing it is noted, not added here.
- No JavaScript framework, no Tailwind, no CSS build pipeline. The single-binary, no-Node constraint is a hard boundary.
- No graph canvas/SVG rendering. The graph stays an indented semantic tree (current `graph_node.html`); this styles that tree, it does not introduce a force-directed view.
- No marketing surfaces. There is no landing page; this is an application reading surface throughout.

## Design

### Aesthetic position

Three references, one resolution:

- Neo Swiss gives the structure: an asymmetric grid with a narrow metadata column and a wide content column, flush-left ragged-right setting, a small number of type sizes, objective neutral tone.
- Industrial Information Design gives the surface treatment: flat, no cards, no drop shadows, hairline rules to group and separate, tabular alignment for data, color used as signal and wayfinding rather than ornament.
- Micro Typography gives the detail: tabular lining numerals everywhere numbers carry meaning, tracking tightened on display sizes and loosened on small caps labels, a 4px baseline grid, a constrained reading measure, hung punctuation where the engine allows.

### Color tokens

Monochrome-dominant. One brand accent, locked across every surface. The lifecycle-status axis is a separate categorical legend, justified below.

Surface and ink ramp (no pure black, no pure white):

| Token              | Light     | Dark      | Role                                      |
| ------------------ | --------- | --------- | ----------------------------------------- |
| `--surface`        | `#fafaf8` | `#0e0f10` | page background (warm paper / near-black) |
| `--surface-raised` | `#ffffff` | `#17181a` | search input, select, inset blocks        |
| `--ink`            | `#161718` | `#eef0ef` | primary text                              |
| `--ink-muted`      | `#5c5f60` | `#9aa0a0` | metadata labels, secondary text           |
| `--ink-faint`      | `#8b8f8f` | `#6b7070` | captions, disabled, placeholder           |
| `--rule`           | `#e2e2dd` | `#2a2c2e` | hairline rules and borders                |
| `--rule-strong`    | `#c7c7c0` | `#3a3d3f` | section dividers, table head rule         |

Brand accent (locked, used for interactive affordance, focus ring, current location, link underline):

| Token           | Light     | Dark      | Role                                           |
| --------------- | --------- | --------- | ---------------------------------------------- |
| `--accent`      | `#e2483d` | `#ff5b4d` | links, focus, active nav, key signal           |
| `--accent-weak` | `#fbe9e7` | `#3a1f1c` | accent background wash (selection, active row) |

The accent is a vermilion red, in the Müller-Brockmann / Swiss-poster lineage and in the industrial safety-signal tradition. It is reserved for interaction and wayfinding, never for status.

Lifecycle-status legend (categorical, label-first, color-redundant). This is a deliberate second color axis, permitted because status is a data dimension being encoded, not decoration. Color is always paired with the mono status label, so the encoding survives for color-blind readers and in monochrome print. Hues are desaturated to coexist with the monochrome base and to stay distinct from the vermilion accent:

| Status      | Token             | Light swatch              | Meaning band        |
| ----------- | ----------------- | ------------------------- | ------------------- |
| draft       | `--st-draft`      | `#9aa0a0` (neutral gray)  | not started         |
| review      | `--st-review`     | `#c98a2b` (amber)         | in flight, gated    |
| accepted    | `--st-accepted`   | `#3f7cc4` (slate blue)    | approved, not built |
| in-progress | `--st-progress`   | `#2f9e6f` (green, hollow) | building            |
| complete    | `--st-complete`   | `#2f9e6f` (green, solid)  | done                |
| rejected    | `--st-rejected`   | `#8a8d8e` (gray, struck)  | closed unbuilt      |
| superseded  | `--st-superseded` | `#8a8d8e` (gray, struck)  | replaced            |

Status is rendered as a mono uppercase micro-label with a 6px leading swatch (or a 1px ring for hollow/in-progress vs solid/complete), not as a filled pill. Rejected and superseded additionally take a strike or reduced opacity, so terminal-dead states read without relying on hue. Document-type (`--st-*` siblings) is encoded by mono label only, no color, to keep the status axis the single colored data dimension.

### Type tokens

Two families. Prose and UI in a neo-grotesque; every machine identifier in a monospace. The mono carries the industrial-information register and gives free tabular alignment for ids, dates, and statuses.

- `--font-sans`: a neo-grotesque. Primary recommendation Neue Haas Grotesk Display / ABC Diatype where licensed; free self-hostable fallback Archivo (variable) or the system Helvetica Neue / -apple-system stack. Inter is the acceptable neutral fallback per the brief, but the grotesque is preferred for Swiss form.
- `--font-mono`: IBM Plex Mono (industrial lineage, strong tabular figures) or Commit Mono / JetBrains Mono. Mono is used for `.doc-id`, `.doc-type`, `.doc-status`, `.doc-date`, `.relation` targets, `.graph-type`, `.graph-status`, and all numerals in metadata.

Modular scale, few steps (Swiss restraint), 1.5 baseline rhythm on body:

| Token         | Size / line-height                          | Use                           |
| ------------- | ------------------------------------------- | ----------------------------- |
| `--t-display` | 28px / 1.15, tracking -0.02em               | document `<h1>` title         |
| `--t-h2`      | 19px / 1.25, tracking -0.01em               | body `<h2>`                   |
| `--t-h3`      | 16px / 1.3                                  | body `<h3>`                   |
| `--t-body`    | 15px / 1.55, measure 68ch                   | prose `.doc-body`             |
| `--t-meta`    | 13px / 1.4 mono                             | frontmatter values, list rows |
| `--t-label`   | 11px / 1.2 mono, uppercase, tracking 0.08em | `<dt>` labels, status, type   |

Micro-typographic rules: `font-variant-numeric: tabular-nums lining` on all mono and metadata; ragged-right, never justified; reading measure capped at 68ch on `.doc-body`; tracking tightened on display, loosened on the uppercase micro-labels; hanging-punctuation where supported; widow/orphan control on prose paragraphs.

### Space and grid tokens

4px base unit; 8px is the dominant rhythm.

| Token    | Value | Use                          |
| -------- | ----- | ---------------------------- |
| `--sp-1` | 4px   | tight pairs (label to value) |
| `--sp-2` | 8px   | default gap                  |
| `--sp-3` | 16px  | block separation             |
| `--sp-4` | 24px  | row padding, list gutters    |
| `--sp-6` | 40px  | section gaps                 |
| `--sp-8` | 64px  | page margin top              |

Layout grid: asymmetric two-column. A narrow left metadata column (`--col-meta: 220px`) and a wide content column (`--col-body: minmax(0, 68ch)`), with the document title spanning both. On viewports below 768px the layout collapses to a single column, metadata stacking above body. The list and graph surfaces use a single full-measure column with a hard outer margin (`--page-pad: clamp(16px, 5vw, 64px)`) and `max-width` 1100px.

### Rule and shape tokens

Industrial flatness. No card containers, no drop shadows.

| Token          | Value                            | Use                                            |
| -------------- | -------------------------------- | ---------------------------------------------- |
| `--rule-w`     | 1px                              | hairline                                       |
| `--radius`     | 0                                | corners are square throughout (one shape lock) |
| `--focus-ring` | 2px solid `--accent`, 2px offset | keyboard focus                                 |

Grouping is done with `border-top` / `divide` hairlines and whitespace, per the density dial. Tinted shadows are not used; if elevation is ever needed it is a `--rule-strong` border, not a shadow. Shape lock: every corner is square (radius 0), consistent with the Swiss/industrial register.

### Motion tokens

| Token            | Value                        | Use                                     |
| ---------------- | ---------------------------- | --------------------------------------- |
| `--motion-swap`  | 120ms ease-out, opacity only | HTMX `innerHTML` swaps (search, filter) |
| `--motion-hover` | 80ms linear, underline/color | link and row hover                      |

No scroll-driven animation, no parallax, no entrance choreography. Everything above trivial collapses to instant under `prefers-reduced-motion: reduce`.

### Component specifications (mapped to existing hooks)

Document page (`doc_page.html`):

- `.doc-frontmatter` becomes the left metadata column: a `<dl>` with `--t-label` uppercase mono `<dt>` in `--ink-muted` and `--t-meta` mono `<dd>` values, each pair on a 4px grid, separated from the body by a `--rule` vertical hairline (horizontal on mobile).
- `<h1>` uses `--t-display`, spanning full width above the two columns, with a thin `--rule-strong` underline.
- `.doc-id` and `.doc-type` render as mono `--t-label`. `.doc-status` renders as the status micro-label + swatch per the legend.
- `.doc-body` is the reading column: `--t-body`, 68ch measure, ragged-right, baseline rhythm. Headings, code, lists, blockquotes styled flat with hairline accents (left `--rule` border on blockquotes, `--surface-raised` inset on code).
- `.relation`, `.doc-parent`, `.doc-children` links: mono, accent underline on the target id, `--ink-muted` for the relation-type prefix.

List page (`list_page.html`, `list_row.html`):

- The `<h1>` and search input sit on a top band separated by `--rule-strong`. Search `<input>` is `--surface-raised`, square, 1px `--rule` border, accent focus ring, mono placeholder in `--ink-faint`.
- `#filters` selects are bare, mono `--t-label`, hairline underline rather than boxed.
- Rows are a tabular list: `.doc-id` (mono, fixed-width gutter), `.doc-title` (sans), `.doc-status` (right-aligned status micro-label). Rows separated by `divide-y` `--rule` hairlines, no cards. Hover washes the row with `--accent-weak`.

Search and filter fragments (`search_fragment.html`): identical row treatment to the list, so HTMX swaps are visually seamless; `.search-empty` / nothing-state in `--ink-faint` italic-free mono.

Graph page (`graph_page.html`, `graph_node.html`):

- The `.graph-tree` is an indented outline. Indentation is driven off `data-depth` with a `--sp-4` step and a connecting `--rule` guide line per level (wayfinding ticks already present in markup: the dotted-arrow relation glyphs stay, colored `--ink-faint`).
- Per node: title (sans), `.graph-type` (mono label, no color), `.graph-status` (status micro-label + swatch), `.graph-related` targets as accent-underlined mono ids.
- `.graph-empty` nothing-state in `--ink-faint`.

### Theming

One token block under `:root` for light, overridden under `@media (prefers-color-scheme: dark)` and a `[data-theme]` attribute hook for an optional manual toggle. Every component references tokens only, never literal colors, so dark parity is automatic. Contrast target WCAG AA for body and metadata, AA-large for the display title; the status legend hues are chosen to clear AA against both surfaces.

## Interfaces

Proposed artifacts (no code in this RFC):

- `@draft` A single static stylesheet served by the web binary (token block plus component rules), referenced from each template `<head>`.
- `@draft` Self-hosted font assets (one variable grotesque, one mono) loaded via `@font-face` with `font-display: swap`, embedded in the binary or served from a static route consistent with ADR-001.
- `@draft` A `<head>` partial or per-template `<link rel="stylesheet">` plus font preload tags. The templates currently have no shared head; introducing one is a markup touch deferred to a story, flagged here.

No engine, CLI, or public API surface changes. This is presentation-layer only, within `src/web/`.

## Decisions (ADRs to emit)

1. Static CSS with custom properties, no build step. Rationale: preserves the single-binary, no-Node architecture (ADR-001); custom properties give the theming contract without a preprocessor.
2. Self-hosted fonts embedded in / served by the binary, versus a system font stack. Tradeoff: binary size and asset embedding against typographic control and offline determinism. Decide the embedding mechanism (compile-time include vs static dir).
3. Status encoded label-first, color-redundant, as a categorical legend distinct from the single locked brand accent. Rationale: status is a data axis, not decoration; redundant encoding preserves the information for color-blind and monochrome readers.
4. Square-corner, card-free, shadow-free surface treatment as the locked shape/material system. Rationale: industrial-information register; density dial forbids card containers.

## Stories

1. Token foundation: the `:root` custom-property block (color, type, space, rule, motion) plus the dark-mode override and font `@font-face` declarations. Blocks all others.
2. Font embedding/serving per ADR decision 2 (depends on 1).
3. Document page styling against `.doc-frontmatter` / `.doc-body` hooks, including the asymmetric two-column grid and mobile collapse (depends on 1).
4. List + search + filter styling with the shared tabular row treatment (depends on 1).
5. Graph tree styling with depth-driven indentation and guide lines (depends on 1).
6. Shared `<head>` partial wiring stylesheet + font preloads into all templates (depends on 1, 2; touches markup).
7. Theming pass: contrast audit in both modes, optional manual toggle hook (depends on 3-5).

## Risks and tradeoffs

- Font embedding inflates binary size. A variable grotesque plus a mono can add hundreds of KB to low MB. Mitigation: subset to Latin, ship a single weight axis, or accept a system-font fallback as the default and treat embedded fonts as an enhancement. This is the main tension with ADR-001's single-binary value.
- Dense mono metadata at `--t-meta` 13px risks failing AA in `--ink-muted`. Mitigation: the contrast pass (story 7) is a gate, not an afterthought; muted tokens are tuned to clear AA before merge.
- Two color axes (accent plus status legend) is the documented exception to one-accent discipline. The risk is drift into decorative color. Mitigation: the rule is that status color only ever appears bound to a status label, and the brand accent only ever signals interaction; any other colored element is a defect.
- Dark-mode status hue parity. Desaturated hues that read on warm paper can muddy on near-black. Mitigation: per-mode status tokens, validated in the contrast pass.
- The shared-head markup change (story 6) is the one place this work touches templates, contradicting the non-goal of zero markup change. It is unavoidable for stylesheet wiring and is scoped narrowly.
