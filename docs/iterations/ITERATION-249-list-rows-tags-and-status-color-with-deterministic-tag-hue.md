---
title: 'List rows: tags and status color with deterministic tag hue'
type: iteration
status: draft
author: jkaloger
date: 2026-07-01
tags: []
related:
- implements: STORY-184
---

## Changes

Web-view only. List rows gain tags + status color; tags get deterministic hue. RFC-053 tag-color delta (per STORY-184).

**1 — `src/web/render.rs`.**
- New `pub struct TagChip { pub name: String, pub hue: u8 }`.
- `DocRow.tags: Vec<TagChip>` (today `{id,title,status}` only).
- `DocPage.tags` → `Vec<TagChip>` (was `Vec<String>`) so doc page + list share hue.
- New `pub fn tag_hue(tag: &str) -> u8`: stable byte hash mod `TAG_HUES` (8). Pure, deterministic, total over any string. Doc: web-only categorical encoding.

**2 — `src/web/routes.rs`.**
- `build_groups` (`:69`) + `search` (`:187`) DocRow: `tags: doc.tags.iter().map(|t| TagChip{name:t.clone(),hue:tag_hue(t)}).collect()`.
- `DocPage::from_doc` (`render.rs:130`) tags map same.

**3 — `templates/list_row.html`.**
Status: `<span class="doc-status">{{doc.status}}</span>` → `<span class="doc-status" data-status="{{doc.status}}"><span class="status-swatch"></span>{{doc.status}}</span>`.
Tags after title: `{% if \!doc.tags.is_empty() %}<span class="doc-tags">{% for t in doc.tags %}<a class="tag tag--h{{t.hue}}" href="/?tag={{t.name}}">{{t.name}}</a>{% if \!loop.last %} {% endif %}{% endfor %}</span>{% endif %}`.

**4 — `templates/doc_page.html`.** tags dd loop: `<a class="tag tag--h{{tag.hue}}" href="/?tag={{tag.name}}">{{tag.name}}</a>` (was `{{tag}}`).

**5 — `static/lazyspec.css`.**
- List `.doc-status` (`:668`): add `display:inline-flex; align-items:center; gap:var(--sp-2)` so swatch shows (hues already keyed `[data-status]` `:258`+).
- Tag hue palette: 8 desaturated tokens `--tag-h0..h7` in `:root` + dark block, distinct from accent + `--st-*`. Classes `.tag--h0{color:var(--tag-h0)} ...`. Applies to `a.tag` (`:867`) + `.doc-tags .tag`. Hover still → accent.
- List row: tags after `.doc-title`, before right-aligned status; mono, small, wrap allowed.

## Test Plan

- `tests/integration/web_serve_test.rs`: list row for tagged doc contains `tag--h` class + `/?tag=` href + tag text; row `.doc-status` has `data-status` + `status-swatch`.
- Unit (`render.rs`): `tag_hue` deterministic (same in → same out), in `0..8`, distinct-ish for sample tags, total on empty/unicode.
- Unknown status → swatch falls back `--ink-faint` (no `[data-status]` rule match) — existing behavior, assert no panic.
- `cargo test --features web` green; `cargo build --features web` clean.

## Notes

- Hue is presentation-only, computed web-layer; engine tag data unchanged → TUI/CLI parity N/A.
- Hash: simple FNV-1a or byte-sum mod 8; collisions acceptable (hue is wayfinding aid, label carries identity per RFC-053 label-first rule).
- Palette desaturated (S<45%) to sit beside mono base + stay clear of vermilion accent and status legend hues.
