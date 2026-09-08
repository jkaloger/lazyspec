use crate::engine::cache::DiskCache;
use crate::engine::config::Config;
use crate::engine::document::DocMeta;
use crate::engine::refs::RefExpander;
use crate::engine::staleness::Staleness;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::{App, AppEvent, StalenessRequest};

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
                let _ = tx.send(AppEvent::ExpansionResult {
                    path: doc_path,
                    body,
                    body_hash,
                });
                return;
            }

            let body_hash = DiskCache::body_hash(&body);

            if let Some(cached) = disk_cache.read(&doc_path, body_hash) {
                let _ = tx.send(AppEvent::ExpansionResult {
                    path: doc_path,
                    body: cached,
                    body_hash,
                });
                return;
            }

            let expander = RefExpander::new(root);
            match expander.expand_cancellable(&body, &cancel) {
                Ok(Some(expanded)) => {
                    let _ = tx.send(AppEvent::ExpansionResult {
                        path: doc_path,
                        body: expanded,
                        body_hash,
                    });
                }
                Ok(None) => {}
                Err(_) => {
                    let _ = tx.send(AppEvent::ExpansionResult {
                        path: doc_path,
                        body,
                        body_hash,
                    });
                }
            }
        });
    }

    /// Dispatch the selected document's staleness band to the background worker
    /// (STORY-275). `compute` shells out to `git diff`, so running it on the
    /// render path would stall every cursor move -- the same reason BUG-011 took
    /// search off the event loop.
    ///
    /// Called once per frame beside [`App::request_expansion`], which is why
    /// none of the eleven-odd ways `selected_doc` moves has to know staleness
    /// exists. The dedupe key is `(path, reviewed)`: a status change stamps
    /// `reviewed` in place (STORY-274), and a path-only key would leave the
    /// pre-stamp band on screen.
    pub fn request_staleness(&mut self, config: &Config) {
        let Some(doc) = self.selected_doc_for_view().cloned() else {
            return;
        };
        let key = (doc.path.clone(), doc.reviewed.clone());
        if self.staleness_key.as_ref() == Some(&key) {
            return;
        }

        self.staleness_key = Some(key);
        self.staleness = None;
        self.staleness_generation = self.staleness_generation.wrapping_add(1);
        let _ = self.staleness_tx.send(StalenessRequest {
            governs_root: self.store.governs_root().to_path_buf(),
            config: config.clone(),
            doc,
            generation: self.staleness_generation,
        });
    }

    /// Apply a worker result, dropping it when the generation has moved on. The
    /// selection may have left the document and come back while git was
    /// running, so a path match is not evidence the result is current -- the
    /// generation is.
    pub fn apply_staleness(&mut self, generation: u64, staleness: Staleness) {
        if generation != self.staleness_generation {
            return;
        }
        self.staleness = Some(staleness);
    }

    /// The band, but only for the document it was computed for. Dispatch runs
    /// once a frame, *after* the draw, so the frame that follows a view switch
    /// or a selection jump renders with the previous document's band still in
    /// the slot. Re-keying by path at the read site is what `expanded_body_cache`
    /// already does, and is what makes AC5 hold on every surface rather than
    /// only on the one dispatch happens to agree with.
    pub fn staleness_for(&self, path: &std::path::Path) -> Option<&Staleness> {
        match &self.staleness_key {
            Some((key_path, _)) if key_path == path => self.staleness.as_ref(),
            _ => None,
        }
    }

    /// Test-only synchronous staleness: dispatch, then compute inline through
    /// `self.git` and apply, so a test exercises the production path without a
    /// worker thread. `App::run_search_now` is the shape.
    #[cfg(test)]
    pub(crate) fn run_staleness_now(&mut self, config: &Config) {
        self.request_staleness(config);
        let Some(doc) = self.selected_doc_for_view().cloned() else {
            return;
        };
        let staleness =
            crate::engine::staleness::compute(self.store.governs_root(), config, &doc, &*self.git);
        self.apply_staleness(self.staleness_generation, staleness);
    }

    pub fn request_diagram_render(
        &mut self,
        block: &crate::tui::content::diagram::DiagramBlock,
        tx: &crossbeam_channel::Sender<AppEvent>,
    ) {
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

            let entry =
                if ascii && block.language == crate::tui::content::diagram::DiagramLanguage::D2 {
                    match crate::tui::content::diagram::render_diagram_text(&block, &cache_dir) {
                        Ok(text) => crate::tui::content::diagram::DiagramCacheEntry::Text(text),
                        Err(err) => {
                            crate::tui::content::diagram::DiagramCacheEntry::Failed(err.to_string())
                        }
                    }
                } else {
                    match crate::tui::content::diagram::render_diagram(&block, &cache_dir) {
                        Ok(path) => crate::tui::content::diagram::DiagramCacheEntry::Image(path),
                        Err(err) => {
                            crate::tui::content::diagram::DiagramCacheEntry::Failed(err.to_string())
                        }
                    }
                };

            let _ = tx.send(AppEvent::DiagramRendered {
                source_hash: hash,
                entry,
            });
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

    /// The document the preview panel actually renders. Both selections are
    /// indexed by `selected_doc`, but the Filters view draws from
    /// `filtered_docs_cache` (every type, status/tag-filtered) and every other
    /// view from the type-scoped `doc_tree`, so the two diverge. Anything
    /// dispatched for "the selection" has to ask which list is on screen;
    /// `open_status_picker` and the link editor already branch the same way.
    pub fn selected_doc_for_view(&mut self) -> Option<&DocMeta> {
        if self.view_mode == crate::tui::state::ViewMode::Filters {
            return self.selected_filtered_doc();
        }
        self.selected_doc_meta()
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
