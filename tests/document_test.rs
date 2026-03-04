use lazyspec::engine::document::{DocMeta, DocType, Status, RelationType};
use chrono::NaiveDate;

#[test]
fn parse_frontmatter_from_markdown() {
    let content = r#"---
title: "Adopt Event Sourcing"
type: adr
status: draft
author: jkaloger
date: 2026-03-04
tags: [architecture, events]
related:
  - implements: rfcs/RFC-001-event-sourcing.md
---

## Context

Some body content here.
"#;

    let meta = DocMeta::parse(content).unwrap();

    assert_eq!(meta.title, "Adopt Event Sourcing");
    assert_eq!(meta.doc_type, DocType::Adr);
    assert_eq!(meta.status, Status::Draft);
    assert_eq!(meta.author, "jkaloger");
    assert_eq!(meta.date, NaiveDate::from_ymd_opt(2026, 3, 4).unwrap());
    assert_eq!(meta.tags, vec!["architecture", "events"]);
    assert_eq!(meta.related.len(), 1);
    assert_eq!(meta.related[0].rel_type, RelationType::Implements);
    assert_eq!(meta.related[0].target, "rfcs/RFC-001-event-sourcing.md");
}

#[test]
fn parse_frontmatter_minimal() {
    let content = r#"---
title: "Simple Doc"
type: rfc
status: review
author: someone
date: 2026-01-01
tags: []
---

Body.
"#;

    let meta = DocMeta::parse(content).unwrap();
    assert_eq!(meta.title, "Simple Doc");
    assert_eq!(meta.doc_type, DocType::Rfc);
    assert!(meta.related.is_empty());
}

#[test]
fn parse_frontmatter_invalid_yaml() {
    let content = "no frontmatter here";
    assert!(DocMeta::parse(content).is_err());
}

#[test]
fn extract_body_skips_frontmatter() {
    let content = r#"---
title: "Test"
type: spec
status: draft
author: a
date: 2026-01-01
tags: []
---

## Body

Content here.
"#;

    let body = DocMeta::extract_body(content).unwrap();
    assert!(body.contains("## Body"));
    assert!(body.contains("Content here."));
    assert!(!body.contains("title:"));
}
