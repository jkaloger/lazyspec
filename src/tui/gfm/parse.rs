use pulldown_cmark::{Alignment, BlockQuoteKind, Event, Options, Parser, Tag, TagEnd};

use super::{GfmSegment, GfmTable};

pub(super) enum ExtractorResult {
    Consumed,
    Finished(std::ops::Range<usize>, GfmSegment),
    FootnoteFinished(std::ops::Range<usize>, GfmSegment),
}

pub(super) trait GfmExtractor {
    fn try_start(&mut self, event: &Event, range: &std::ops::Range<usize>) -> bool;
    fn feed(&mut self, event: &Event, range: &std::ops::Range<usize>) -> Option<ExtractorResult>;
    fn active(&self) -> bool;
}

pub(super) struct TableExtractor {
    active: bool,
    alignments: Vec<Alignment>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    cell_text: String,
    in_head: bool,
    start_offset: usize,
}

impl TableExtractor {
    pub(super) fn new() -> Self {
        Self {
            active: false,
            alignments: Vec::new(),
            headers: Vec::new(),
            rows: Vec::new(),
            current_row: Vec::new(),
            cell_text: String::new(),
            in_head: false,
            start_offset: 0,
        }
    }
}

impl GfmExtractor for TableExtractor {
    fn try_start(&mut self, event: &Event, range: &std::ops::Range<usize>) -> bool {
        let Event::Start(Tag::Table(aligns)) = event else {
            return false;
        };
        self.active = true;
        self.alignments = aligns.clone();
        self.start_offset = range.start;
        true
    }

    fn feed(&mut self, event: &Event, range: &std::ops::Range<usize>) -> Option<ExtractorResult> {
        if !self.active {
            return None;
        }
        match event {
            Event::Start(Tag::TableHead) => self.in_head = true,
            Event::End(TagEnd::TableHead) => self.in_head = false,
            Event::Start(Tag::TableRow) => self.current_row.clear(),
            Event::End(TagEnd::TableRow) => {
                self.rows.push(self.current_row.clone());
                self.current_row.clear();
            }
            Event::Start(Tag::TableCell) => self.cell_text.clear(),
            Event::End(TagEnd::TableCell) => {
                if self.in_head {
                    self.headers.push(self.cell_text.clone());
                } else {
                    self.current_row.push(self.cell_text.clone());
                }
                self.cell_text.clear();
            }
            Event::Text(t) | Event::Code(t) => self.cell_text.push_str(t),
            Event::SoftBreak | Event::HardBreak => self.cell_text.push(' '),
            Event::End(TagEnd::Table) => {
                let seg = GfmSegment::Table(GfmTable {
                    headers: self.headers.clone(),
                    alignments: self.alignments.clone(),
                    rows: self.rows.clone(),
                });
                let result = ExtractorResult::Finished(self.start_offset..range.end, seg);
                self.active = false;
                self.headers.clear();
                self.alignments.clear();
                self.rows.clear();
                self.current_row.clear();
                return Some(result);
            }
            _ => {}
        }
        Some(ExtractorResult::Consumed)
    }

    fn active(&self) -> bool {
        self.active
    }
}

pub(super) struct AdmonitionExtractor {
    kind: Option<String>,
    body: String,
    depth: usize,
    start_offset: usize,
}

impl AdmonitionExtractor {
    pub(super) fn new() -> Self {
        Self {
            kind: None,
            body: String::new(),
            depth: 0,
            start_offset: 0,
        }
    }
}

impl GfmExtractor for AdmonitionExtractor {
    fn try_start(&mut self, event: &Event, range: &std::ops::Range<usize>) -> bool {
        let Event::Start(Tag::BlockQuote(Some(kind))) = event else {
            return false;
        };
        let kind_str = match kind {
            BlockQuoteKind::Note => "Note",
            BlockQuoteKind::Warning => "Warning",
            BlockQuoteKind::Tip => "Tip",
            BlockQuoteKind::Important => "Important",
            BlockQuoteKind::Caution => "Caution",
        };
        self.kind = Some(kind_str.to_string());
        self.depth = 0;
        self.start_offset = range.start;
        true
    }

    fn feed(&mut self, event: &Event, range: &std::ops::Range<usize>) -> Option<ExtractorResult> {
        self.kind.as_ref()?;
        match event {
            Event::Start(Tag::BlockQuote(_)) => self.depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => {
                if self.depth == 0 {
                    let seg = GfmSegment::Admonition {
                        kind: self.kind.take().unwrap(),
                        body: self.body.trim().to_string(),
                    };
                    let result = ExtractorResult::Finished(self.start_offset..range.end, seg);
                    self.body.clear();
                    return Some(result);
                }
                self.depth -= 1;
            }
            Event::Text(t) | Event::Code(t) => self.body.push_str(t),
            Event::SoftBreak | Event::HardBreak => self.body.push('\n'),
            _ => {}
        }
        Some(ExtractorResult::Consumed)
    }

    fn active(&self) -> bool {
        self.kind.is_some()
    }
}

pub(super) struct FootnoteExtractor {
    active: bool,
    label: String,
    body: String,
    start_offset: usize,
}

impl FootnoteExtractor {
    pub(super) fn new() -> Self {
        Self {
            active: false,
            label: String::new(),
            body: String::new(),
            start_offset: 0,
        }
    }
}

impl GfmExtractor for FootnoteExtractor {
    fn try_start(&mut self, event: &Event, range: &std::ops::Range<usize>) -> bool {
        let Event::Start(Tag::FootnoteDefinition(label)) = event else {
            return false;
        };
        self.active = true;
        self.label = label.to_string();
        self.start_offset = range.start;
        true
    }

    fn feed(&mut self, event: &Event, range: &std::ops::Range<usize>) -> Option<ExtractorResult> {
        if !self.active {
            return None;
        }
        match event {
            Event::End(TagEnd::FootnoteDefinition) => {
                let seg = GfmSegment::Footnote {
                    label: self.label.clone(),
                    body: self.body.trim().to_string(),
                };
                let result = ExtractorResult::FootnoteFinished(self.start_offset..range.end, seg);
                self.active = false;
                self.label.clear();
                self.body.clear();
                return Some(result);
            }
            Event::Text(t) | Event::Code(t) => self.body.push_str(t),
            Event::SoftBreak | Event::HardBreak => self.body.push('\n'),
            _ => {}
        }
        Some(ExtractorResult::Consumed)
    }

    fn active(&self) -> bool {
        self.active
    }
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_GFM
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

pub fn extract_gfm_segments(body: &str) -> Vec<GfmSegment> {
    let parser = Parser::new_ext(body, parser_options());
    let offset_iter = parser.into_offset_iter();

    let mut ranged_segments: Vec<(std::ops::Range<usize>, GfmSegment)> = Vec::new();
    let mut footnotes: Vec<GfmSegment> = Vec::new();
    let mut footnote_ranges: Vec<std::ops::Range<usize>> = Vec::new();

    let mut table_ext = TableExtractor::new();
    let mut admonition_ext = AdmonitionExtractor::new();
    let mut footnote_ext = FootnoteExtractor::new();

    for (event, range) in offset_iter {
        let extractors: [&mut dyn GfmExtractor; 3] = [
            &mut footnote_ext,
            &mut admonition_ext,
            &mut table_ext,
        ];

        let mut handled = false;
        for ext in extractors {
            if !ext.active() {
                continue;
            }
            match ext.feed(&event, &range) {
                Some(ExtractorResult::Consumed) => {
                    handled = true;
                    break;
                }
                Some(ExtractorResult::Finished(r, seg)) => {
                    ranged_segments.push((r, seg));
                    handled = true;
                    break;
                }
                Some(ExtractorResult::FootnoteFinished(r, seg)) => {
                    footnote_ranges.push(r);
                    footnotes.push(seg);
                    handled = true;
                    break;
                }
                None => {}
            }
        }

        if handled {
            continue;
        }

        let _ = table_ext.try_start(&event, &range)
            || admonition_ext.try_start(&event, &range)
            || footnote_ext.try_start(&event, &range);
    }

    assemble_segments(body, ranged_segments, footnote_ranges, footnotes)
}

fn assemble_segments(
    body: &str,
    mut ranged_segments: Vec<(std::ops::Range<usize>, GfmSegment)>,
    mut footnote_ranges: Vec<std::ops::Range<usize>>,
    footnotes: Vec<GfmSegment>,
) -> Vec<GfmSegment> {
    ranged_segments.sort_by_key(|(r, _)| r.start);
    footnote_ranges.sort_by_key(|r| r.start);

    let mut all_ranges: Vec<std::ops::Range<usize>> = ranged_segments
        .iter()
        .map(|(r, _)| r.clone())
        .chain(footnote_ranges.iter().cloned())
        .collect();
    all_ranges.sort_by_key(|r| r.start);

    let mut result: Vec<GfmSegment> = Vec::new();
    let mut cursor = 0;
    let mut seg_idx = 0;

    for r in &all_ranges {
        if cursor < r.start {
            let md = body[cursor..r.start].trim();
            if !md.is_empty() {
                result.push(GfmSegment::Markdown(md.to_string()));
            }
        }
        if seg_idx < ranged_segments.len() && ranged_segments[seg_idx].0 == *r {
            result.push(ranged_segments[seg_idx].1.clone());
            seg_idx += 1;
        }
        cursor = r.end;
    }

    if cursor < body.len() {
        let md = body[cursor..].trim();
        if !md.is_empty() {
            result.push(GfmSegment::Markdown(md.to_string()));
        }
    }

    result.extend(footnotes);

    if result.is_empty() && !body.trim().is_empty() {
        result.push(GfmSegment::Markdown(body.trim().to_string()));
    }

    result
}
