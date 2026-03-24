use crate::engine::cache::DiskCache;
use crate::engine::document::DocMeta;
use crate::engine::refs::RefExpander;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::{App, AppEvent};

impl App {
    pub fn request_expansion(&mut self, tx: &crossbeam_channel::Sender<AppEvent>) {
        let doc_path = match self.selected_doc_meta() {
            Some(meta) => meta.path.clone(),
            None => return,
        };

        if self.expanded_body_cache.contains_key(&doc_path) {
            return;
        }

        if self.expansion_in_flight.as_ref() == Some(&doc_path) {
            return;
        }

        if let Some(cancel) = &self.expansion_cancel {
            cancel.store(true, Ordering::Relaxed);
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.expansion_cancel = Some(cancel.clone());
        self.expansion_in_flight = Some(doc_path.clone());

        let root = self.store.root().to_path_buf();
        let tx = tx.clone();
        let disk_cache = self.disk_cache.clone();
        std::thread::spawn(move || {
            let full_path = root.join(&doc_path);
            let content = match fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => return,
            };
            let body = match DocMeta::extract_body(&content) {
                Ok(b) => b,
                Err(_) => return,
            };

            if !body.contains("@ref ") {
                let body_hash = DiskCache::body_hash(&body);
                let _ = tx.send(AppEvent::ExpansionResult { path: doc_path, body, body_hash });
                return;
            }

            let body_hash = DiskCache::body_hash(&body);

            if let Some(cached) = disk_cache.read(&doc_path, body_hash) {
                let _ = tx.send(AppEvent::ExpansionResult { path: doc_path, body: cached, body_hash });
                return;
            }

            let expander = RefExpander::new(root);
            match expander.expand_cancellable(&body, &cancel) {
                Ok(Some(expanded)) => {
                    let _ = tx.send(AppEvent::ExpansionResult { path: doc_path, body: expanded, body_hash });
                }
                Ok(None) => {}
                Err(_) => {
                    let _ = tx.send(AppEvent::ExpansionResult { path: doc_path, body, body_hash });
                }
            }
        });
    }

    pub fn request_diagram_render(&mut self, block: &crate::tui::content::diagram::DiagramBlock, tx: &crossbeam_channel::Sender<AppEvent>) {
        let hash = crate::tui::content::diagram::source_hash(&block.source);

        if self.diagram_cache.get(hash).is_some() {
            return;
        }

        if !self.tool_availability.is_available(&block.language) {
            return;
        }

        self.diagram_cache.mark_rendering(hash);

        let source = block.source.clone();
        let language = block.language.clone();
        let cache_dir = self.diagram_cache.cache_dir().to_path_buf();
        let tx = tx.clone();
        let ascii = self.ascii_diagrams;

        std::thread::spawn(move || {
            let block = crate::tui::content::diagram::DiagramBlock {
                language,
                source,
                byte_range: 0..0,
            };

            let entry = if ascii && block.language == crate::tui::content::diagram::DiagramLanguage::D2 {
                match crate::tui::content::diagram::render_diagram_text(&block, &cache_dir) {
                    Ok(text) => crate::tui::content::diagram::DiagramCacheEntry::Text(text),
                    Err(err) => crate::tui::content::diagram::DiagramCacheEntry::Failed(err.to_string()),
                }
            } else {
                match crate::tui::content::diagram::render_diagram(&block, &cache_dir) {
                    Ok(path) => crate::tui::content::diagram::DiagramCacheEntry::Image(path),
                    Err(err) => crate::tui::content::diagram::DiagramCacheEntry::Failed(err.to_string()),
                }
            };

            let _ = tx.send(AppEvent::DiagramRendered { source_hash: hash, entry });
        });
    }

    pub fn filtered_docs(&mut self) -> Vec<&DocMeta> {
        use crate::engine::store::Filter;

        if self.filtered_docs_cache.is_none() {
            let mut docs = self.store.list(&Filter {
                doc_type: None,
                status: self.filter_status.clone(),
                tag: self.filter_tag.clone(),
            });
            docs.sort_by(|a, b| DocMeta::sort_by_date(a, b));
            self.filtered_docs_cache = Some(docs.iter().map(|d| d.path.clone()).collect());
        }
        self.filtered_docs_cache
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|p| self.store.get(p))
            .collect()
    }

    pub fn filtered_docs_count(&mut self) -> usize {
        if self.filtered_docs_cache.is_none() {
            self.filtered_docs();
        }
        self.filtered_docs_cache.as_ref().map_or(0, |c| c.len())
    }

    pub fn selected_filtered_doc(&mut self) -> Option<&DocMeta> {
        if self.filtered_docs_cache.is_none() {
            self.filtered_docs();
        }
        self.filtered_docs_cache
            .as_deref()
            .unwrap_or_default()
            .get(self.selected_doc)
            .and_then(|p| self.store.get(p))
    }
}
