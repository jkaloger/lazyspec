# Lazyspec web view design spec

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

Micro-typographic rules: `font-variant-numeric: tabular-nums lining` on all mono and metadata; ragged-right, never justified; reading measure capped at 68ch on `.doc-body`; tracking tightened on display, loosened on the uppercase micro-labels; hanging-punctuation where supported; widow/orphan control on prose paragraphs. Additional detail register: `font-variant-numeric: oldstyle-nums` is never used (data-tool, not editorial); slashed zero on the mono via `font-feature-settings: "zero" 1` so `0` and `O` never collide in ids; superior figures (`sups`) for footnote and relation-count markers; small-caps (`smcp`) for the `<dt>` labels where the chosen grotesque ships them, falling back to the uppercase mono micro-label otherwise.

### Data marks and micro-infographics

The industrial-information register is carried not only by hairlines and tabular figures but by a small, fixed vocabulary of data marks. Discipline: a mark is permitted only when it encodes a datum the engine already holds. Nothing here is ornament. A barcode that does not scan, a QR that links nowhere, a sparkline from invented numbers, crop marks or fiducials drawn for atmosphere — all banned. Each mark must survive monochrome print and degrade to a plain label when the datum is absent.

| Mark | Encodes | Where | Honest form |
| ---- | ------- | ----- | ----------- |
| `.spine` lifecycle track | the type's status DAG with the current status filled | doc header, list row left edge | a segmented horizontal track, one cell per lifecycle status, cells before current filled in `--ink`, current in `--accent`, future in `--rule`; the mono status label sits adjacent so the color is redundant. This is the status legend rendered positionally. |
| `.code128` id strip | the `.doc-id` (e.g. `ITERATION-246`), Code 128-B | doc header, print stylesheet | a genuinely scannable Code 128 rendered from the id at 1px module width, max 28px tall, `--ink` on `--surface`. Functional: scans back to the id for physical-to-digital handoff. Screen-optional, print-default. |
| `.datamatrix` deep link | the doc's canonical URL or git path | doc footer, print only | a Data Matrix (denser than QR at small sizes, no quiet-zone waste) sized to ~20mm in print, hidden on screen via `@media screen`. Resolves to the live doc. |
| `.relbars` fan-in/out | inbound vs outbound relation counts | doc metadata column | two stacked micro-bars, tabular count printed at the end of each, scaled against the max degree in the working set so the bars are comparable across docs. |
| `.heat` activity strip | commit/edit timestamps over the doc's life | doc metadata column | a row of fixed-width cells, one per week since creation, tinted by edit count in a single-hue `--ink` ramp (no rainbow). A true small-multiple, not a gradient. |

Rendering rules for all marks: built as inline SVG or CSS from engine data, never as a raster image or a hand-traced path; `--radius: 0` applies (square modules, square cells); they live in the metadata column or header strip, never inside prose; reduced-motion is irrelevant (all static); each carries an accessible text equivalent (the bare datum) so the mark is progressive enhancement over the label, not a replacement for it. The `--accent` vermilion appears in marks only to denote *current position* (the filled spine cell), consistent with its wayfinding role everywhere else; status hues from the categorical legend never bleed into these marks.

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
