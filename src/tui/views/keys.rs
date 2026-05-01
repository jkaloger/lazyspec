use crate::engine::config::Config;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::Path;

#[cfg(feature = "agent")]
use crate::tui::agent::AgentStatus;

use crate::tui::state::forms::REL_TYPES;
use crate::tui::state::{App, FilterField, PreviewTab, ScopeInputMode, ViewMode};

impl App {
    pub fn handle_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        root: &Path,
        config: &Config,
    ) {
        if self.gh_conflict_message.is_some() {
            if code == KeyCode::Esc {
                self.gh_conflict_message = None;
            }
            return;
        }
        if self.show_help {
            self.show_help = false;
            return;
        }
        if self.show_warnings {
            return self.handle_warnings_key(code);
        }
        if self.create_form.active {
            return self.handle_create_form_key(code, root, config);
        }
        if self.delete_confirm.active {
            return self.handle_delete_confirm_key(code, root, config);
        }
        if self.status_picker.active {
            return self.handle_status_picker_key(code, root, config);
        }
        if self.link_editor.active {
            return self.handle_link_editor_key(code, root, config);
        }
        if self.provenance_editor.active {
            return self.handle_provenance_editor_key(code, root, config);
        }
        #[cfg(feature = "agent")]
        if self.agent_dialog.active {
            return self.handle_agent_dialog_key(code, config);
        }
        if self.search_mode {
            return self.handle_search_key(code, modifiers);
        }
        if self.fullscreen_doc {
            return self.handle_fullscreen_key(code, modifiers);
        }
        self.handle_normal_key(code, modifiers, root, config);
    }

    fn handle_create_form_key(&mut self, code: KeyCode, root: &Path, config: &Config) {
        if self.create_form.loading {
            if code == KeyCode::Esc {
                self.close_create_form();
            }
            return;
        }
        match code {
            KeyCode::Esc => self.close_create_form(),
            KeyCode::Enter => {
                let _ = self.submit_create_form(root, config);
            }
            KeyCode::Tab => self.form_next_field(),
            KeyCode::BackTab => self.form_prev_field(),
            KeyCode::Backspace => self.form_backspace(),
            KeyCode::Char(c) => self.form_type_char(c),
            _ => {}
        }
    }

    fn handle_delete_confirm_key(&mut self, code: KeyCode, root: &Path, config: &Config) {
        match code {
            KeyCode::Enter => {
                let _ = self.confirm_delete(root, config);
            }
            KeyCode::Esc => self.close_delete_confirm(),
            _ => {}
        }
    }

    fn handle_status_picker_key(&mut self, code: KeyCode, root: &Path, config: &Config) {
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.status_picker.selected < 6 {
                    self.status_picker.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.status_picker.selected > 0 {
                    self.status_picker.selected -= 1;
                }
            }
            KeyCode::Enter => {
                let _ = self.confirm_status_change(root, config);
            }
            KeyCode::Esc => self.close_status_picker(),
            _ => {}
        }
    }

    pub(crate) fn handle_link_editor_key(&mut self, code: KeyCode, root: &Path, config: &Config) {
        match code {
            KeyCode::Esc => self.close_link_editor(),
            KeyCode::Tab => {
                self.link_editor.rel_type_index =
                    (self.link_editor.rel_type_index + 1) % REL_TYPES.len();
            }
            KeyCode::Enter => {
                if !self.link_editor.results.is_empty() {
                    let _ = self.confirm_link(root, config);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.link_editor.results.is_empty() {
                    let max = self.link_editor.results.len() - 1;
                    self.link_editor.selected = (self.link_editor.selected + 1).min(max);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.link_editor.selected = self.link_editor.selected.saturating_sub(1);
            }
            KeyCode::Backspace => {
                self.link_editor.query.pop();
                self.update_link_search();
            }
            KeyCode::Char(c) => {
                self.link_editor.query.push(c);
                self.update_link_search();
            }
            _ => {}
        }
    }

    fn handle_provenance_editor_key(&mut self, code: KeyCode, root: &Path, config: &Config) {
        match code {
            KeyCode::Esc => self.close_provenance_editor(),
            KeyCode::Enter => {
                let _ = self.submit_provenance(root, config);
            }
            KeyCode::Backspace => self.provenance_backspace(),
            KeyCode::Char(c) => self.provenance_type_char(c),
            _ => {}
        }
    }

    fn handle_warnings_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Char('w') | KeyCode::Char('q') => self.close_warnings(),
            KeyCode::Char('f') => {
                self.fix_request = true;
            }
            KeyCode::Char('j') | KeyCode::Down => self.warnings_move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.warnings_move_up(),
            _ => {}
        }
    }

    #[cfg(feature = "agent")]
    fn handle_agent_dialog_key(&mut self, code: KeyCode, config: &Config) {
        if self.agent_dialog.text_input.is_some() {
            self.handle_agent_text_input_key(code);
            return;
        }

        match code {
            KeyCode::Esc => {
                self.agent_dialog.active = false;
            }
            KeyCode::Up => {
                if self.agent_dialog.selected_index > 0 {
                    self.agent_dialog.selected_index -= 1;
                } else {
                    self.agent_dialog.selected_index =
                        self.agent_dialog.actions.len().saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if self.agent_dialog.actions.is_empty() {
                    return;
                }
                self.agent_dialog.selected_index =
                    (self.agent_dialog.selected_index + 1) % self.agent_dialog.actions.len();
            }
            KeyCode::Enter => {
                let action = self
                    .agent_dialog
                    .actions
                    .get(self.agent_dialog.selected_index)
                    .cloned()
                    .unwrap_or_default();
                let doc_path = self.agent_dialog.doc_path.clone();

                if action == "Custom prompt" {
                    self.agent_dialog.text_input = Some(String::new());
                    return;
                }

                self.agent_dialog.active = false;

                let doc_title = self.agent_dialog.doc_title.clone();

                if action == "Expand document" {
                    let full_path = self.store.root.join(&doc_path);
                    if let Ok(content) = self.fs.read_to_string(&full_path) {
                        let prompt = crate::tui::agent::build_expand_prompt(&content, &full_path);
                        let _ = self
                            .agent_spawner
                            .spawn(&prompt, &full_path, &doc_title, &action);
                    }
                } else if action == "Create children" {
                    self.spawn_create_children(&doc_path, &doc_title, config);
                }
            }
            _ => {}
        }
    }

    #[cfg(feature = "agent")]
    fn spawn_create_children(&mut self, doc_path: &Path, doc_title: &str, config: &Config) {
        let doc = match self.store.get(doc_path) {
            Some(d) => d,
            None => return,
        };
        let doc_type_str = doc.doc_type.to_string();
        let child_type = config.rules.iter().find_map(|rule| match rule {
            crate::engine::config::ValidationRule::ParentChild { parent, child, .. }
                if parent == &doc_type_str =>
            {
                Some(child.clone())
            }
            _ => None,
        });
        let child_type = match child_type {
            Some(ct) => ct,
            None => return,
        };
        let full_path = self.store.root.join(doc_path);
        let content = match self.fs.read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let prompt = crate::tui::agent::build_create_children_prompt(&content, &child_type);
        let _ = self
            .agent_spawner
            .spawn(&prompt, &full_path, doc_title, "Create children");
    }

    #[cfg(feature = "agent")]
    fn handle_agent_text_input_key(&mut self, code: KeyCode) {
        let buffer = match self.agent_dialog.text_input.as_mut() {
            Some(b) => b,
            None => return,
        };

        match code {
            KeyCode::Esc => {
                self.agent_dialog.text_input = None;
            }
            KeyCode::Enter => {
                let prompt = buffer.clone();
                let full_path = self.store.root.join(&self.agent_dialog.doc_path);
                self.agent_dialog.active = false;
                self.agent_dialog.text_input = None;

                if !prompt.is_empty() {
                    let doc_title = self.agent_dialog.doc_title.clone();
                    if let Ok(content) = self.fs.read_to_string(&full_path) {
                        let full_prompt = format!(
                            "Here is the document:\n\n{}\n\nUser request: {}",
                            content, prompt
                        );
                        let _ = self.agent_spawner.spawn(
                            &full_prompt,
                            &full_path,
                            &doc_title,
                            "Custom prompt",
                        );
                    }
                }
            }
            KeyCode::Backspace => {
                buffer.pop();
            }
            KeyCode::Char(c) => {
                buffer.push(c);
            }
            _ => {}
        }
    }

    fn handle_search_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Esc => self.exit_search(),
            KeyCode::Enter => self.select_search_result(),
            KeyCode::Backspace => {
                self.search_query.pop();
                self.update_search();
            }
            KeyCode::Up => self.search_move_up(),
            KeyCode::Down => self.search_move_down(),
            KeyCode::Char(c) => {
                if modifiers.contains(KeyModifiers::CONTROL) && c == 'k' {
                    self.search_move_up();
                } else if modifiers.contains(KeyModifiers::CONTROL) && c == 'j' {
                    self.search_move_down();
                } else {
                    self.search_query.push(c);
                    self.update_search();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_fullscreen_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('q'), _) => self.exit_fullscreen(),
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => self.scroll_down(),
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => self.scroll_up(),
            (KeyCode::Char('g'), _) => self.scroll_offset = 0,
            (KeyCode::Char('G'), _) => self.scroll_offset = u16::MAX / 2,
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                self.scroll_offset = self
                    .scroll_offset
                    .saturating_add(self.fullscreen_height as u16 / 2);
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.scroll_offset = self
                    .scroll_offset
                    .saturating_sub(self.fullscreen_height as u16 / 2);
            }
            _ => {}
        }
    }

    #[cfg(feature = "agent")]
    fn handle_agents_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let record_count = self.agent_spawner.records.len();

        if modifiers.contains(KeyModifiers::CONTROL) {
            match code {
                KeyCode::Char('d') => {
                    let jump = self.doc_list_height / 2;
                    self.agent_selected_index =
                        (self.agent_selected_index + jump).min(record_count.saturating_sub(1));
                }
                KeyCode::Char('u') => {
                    let jump = self.doc_list_height / 2;
                    self.agent_selected_index = self.agent_selected_index.saturating_sub(jump);
                }
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.agent_selected_index =
                    (self.agent_selected_index + 1).min(record_count.saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.agent_selected_index = self.agent_selected_index.saturating_sub(1);
            }
            KeyCode::Char('e') => {
                if record_count > 0 {
                    let doc_path = &self.agent_spawner.records[self.agent_selected_index].doc_path;
                    self.editor_request = Some(self.store.root.join(doc_path));
                }
            }
            KeyCode::Char('r') => {
                if record_count > 0 {
                    let record = &self.agent_spawner.records[self.agent_selected_index];
                    if record.status != AgentStatus::Running {
                        self.resume_request = Some(record.session_id.clone());
                    }
                }
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('`') => {
                self.cycle_mode();
            }
            _ => {}
        }
    }

    fn handle_filters_key(&mut self, code: KeyCode, modifiers: KeyModifiers, root: &Path) {
        if modifiers.contains(KeyModifiers::CONTROL) {
            match code {
                KeyCode::Char('d') => {
                    let count = self.filtered_docs_count();
                    self.half_page_down(count);
                }
                KeyCode::Char('u') => {
                    let count = self.filtered_docs_count();
                    self.half_page_up(count);
                }
                _ => {}
            }
            return;
        }
        match code {
            KeyCode::Tab => {
                self.filter_focused = self.filter_focused.next();
            }
            KeyCode::BackTab => {
                self.filter_focused = self.filter_focused.prev();
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.cycle_filter_value_prev();
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.cycle_filter_value_next();
            }
            KeyCode::Enter if self.filter_focused == FilterField::ClearAction => {
                self.reset_filters();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let count = self.filtered_docs_count();
                if count > 0 && self.selected_doc < count - 1 {
                    self.selected_doc += 1;
                }
                let count = self.filtered_docs_count();
                self.adjust_viewport(count);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_doc > 0 {
                    self.selected_doc -= 1;
                }
                let count = self.filtered_docs_count();
                self.adjust_viewport(count);
            }
            KeyCode::Enter => {
                if self.preview_tab == PreviewTab::Relations {
                    self.navigate_to_relation();
                } else if self.selected_filtered_doc().is_some() {
                    self.fullscreen_doc = true;
                    self.scroll_offset = 0;
                }
            }
            KeyCode::Char('g') => {
                self.selected_doc = 0;
                self.doc_list_offset = 0;
            }
            KeyCode::Char('G') => {
                let count = self.filtered_docs_count();
                if count > 0 {
                    self.selected_doc = count - 1;
                    self.doc_list_offset = count.saturating_sub(self.doc_list_height);
                }
            }
            KeyCode::Char('e') => {
                if let Some(doc) = self.selected_filtered_doc() {
                    self.editor_request = Some(root.join(&doc.path));
                }
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('`') => {
                self.cycle_mode();
            }
            KeyCode::Char('?') => {
                self.show_help = true;
            }
            KeyCode::Char('/') => {
                self.enter_search();
            }
            KeyCode::Char('w') => {
                self.open_warnings();
            }
            KeyCode::Char('s') => {
                self.open_status_picker();
            }
            _ => {}
        }
    }

    fn handle_graph_key(&mut self, code: KeyCode, _modifiers: KeyModifiers, root: &Path) {
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.graph_selected =
                    (self.graph_selected + 1).min(self.graph_nodes.len().saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.graph_selected = self.graph_selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(node) = self.graph_nodes.get(self.graph_selected) {
                    let path = node.path.clone();
                    if let Some(doc) = self.store.get(&path) {
                        let doc_type = doc.doc_type.clone();
                        if let Some(type_idx) = self.doc_types.iter().position(|t| *t == doc_type) {
                            self.selected_type = type_idx;
                            self.build_doc_tree();
                            if let Some(doc_idx) = self.doc_tree.iter().position(|n| n.path == path)
                            {
                                self.selected_doc = doc_idx;
                            }
                        }
                    }
                    self.view_mode = ViewMode::Types;
                }
            }
            KeyCode::Char('g') => {
                self.graph_selected = 0;
            }
            KeyCode::Char('G') => {
                self.graph_selected = self.graph_nodes.len().saturating_sub(1);
            }
            KeyCode::Char('e') => {
                if let Some(node) = self.graph_nodes.get(self.graph_selected) {
                    self.editor_request = Some(root.join(&node.path));
                }
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('`') => {
                self.cycle_mode();
            }
            _ => {}
        }
    }

    fn handle_sequencing_key(&mut self, code: KeyCode) {
        // If a scope-input is in progress, route keys to the input.
        if self.sequencing.scope_input.is_some() {
            match code {
                KeyCode::Esc => {
                    self.sequencing.scope_input = None;
                }
                KeyCode::Enter => {
                    let (mode, buf) = self.sequencing.scope_input.take().unwrap();
                    let id = buf.trim().to_string();
                    if id.is_empty() {
                        return;
                    }
                    match mode {
                        ScopeInputMode::Under => self.sequencing.set_scope_under(&id, &self.store),
                        ScopeInputMode::After => self.sequencing.set_scope_after(&id, &self.store),
                    }
                }
                KeyCode::Backspace => {
                    if let Some((_, buf)) = self.sequencing.scope_input.as_mut() {
                        buf.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if let Some((_, buf)) = self.sequencing.scope_input.as_mut() {
                        buf.push(c);
                    }
                }
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Char('s') => {
                self.sequencing.scope_input = Some((ScopeInputMode::Under, String::new()));
                self.sequencing.error = None;
                self.sequencing.info = None;
            }
            KeyCode::Char('f') => {
                self.sequencing.scope_input = Some((ScopeInputMode::After, String::new()));
                self.sequencing.error = None;
                self.sequencing.info = None;
            }
            KeyCode::Char('c') => {
                self.sequencing.clear_scope(&self.store);
                self.sequencing.info = None;
            }
            // Edit gestures are deferred (STORY-119); surface a read-only notice.
            KeyCode::Char('a')
            | KeyCode::Char('d')
            | KeyCode::Char('1')
            | KeyCode::Char('2')
            | KeyCode::Char('3') => {
                self.sequencing.info = Some("read-only screen".to_string());
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('`') => {
                self.cycle_mode();
            }
            _ => {}
        }
    }

    #[allow(unused_variables)]
    fn handle_normal_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        root: &Path,
        config: &Config,
    ) {
        match self.view_mode {
            ViewMode::Filters => return self.handle_filters_key(code, modifiers, root),
            ViewMode::Graph => return self.handle_graph_key(code, modifiers, root),
            ViewMode::Sequencing => return self.handle_sequencing_key(code),
            #[cfg(feature = "agent")]
            ViewMode::Agents => return self.handle_agents_key(code, modifiers),
            _ => {}
        }

        match (code, modifiers) {
            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            (KeyCode::Char('?'), _) => {
                self.show_help = true;
            }
            (KeyCode::Char('/'), _) => self.enter_search(),
            (KeyCode::Char('n'), _) => self.open_create_form(),
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                let count = self.doc_tree.len();
                self.half_page_down(count);
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                let count = self.doc_tree.len();
                self.half_page_up(count);
            }
            (KeyCode::Char('d'), _) if self.selected_doc_meta().is_some() => {
                self.open_delete_confirm();
            }
            (KeyCode::Char('e'), _) if self.selected_doc_meta().is_some() => {
                if let Some(doc) = self.selected_doc_meta() {
                    self.editor_request = Some(root.join(&doc.path));
                }
            }
            (KeyCode::Enter, _) => {
                if self.preview_tab == PreviewTab::Relations {
                    self.navigate_to_relation();
                } else {
                    self.enter_fullscreen();
                }
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.preview_tab == PreviewTab::Relations {
                    self.move_relation_down();
                } else {
                    self.move_down();
                }
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                if self.preview_tab == PreviewTab::Relations {
                    self.move_relation_up();
                } else {
                    self.move_up();
                }
            }
            (KeyCode::Char('l') | KeyCode::Right, _) => {
                self.move_type_next();
            }
            (KeyCode::Char('h') | KeyCode::Left, _) => {
                self.move_type_prev();
            }
            (KeyCode::Char(' '), _) => {
                let node = self.doc_tree.get(self.selected_doc).cloned();
                if let Some(ref n) = node {
                    if n.is_parent && !self.is_expanded(&n.path) {
                        let path = n.path.clone();
                        self.toggle_expanded(&path);
                    } else if n.is_parent && self.is_expanded(&n.path) {
                        let path = n.path.clone();
                        self.toggle_expanded(&path);
                        self.clamp_selected_doc();
                    } else if n.depth > 0 {
                        let mut parent_idx = self.selected_doc;
                        for i in (0..self.selected_doc).rev() {
                            if self.doc_tree[i].depth == 0 {
                                parent_idx = i;
                                break;
                            }
                        }
                        self.selected_doc = parent_idx;
                        let path = self.doc_tree[parent_idx].path.clone();
                        if self.is_expanded(&path) {
                            self.toggle_expanded(&path);
                            self.clamp_selected_doc();
                        }
                    }
                }
            }
            (KeyCode::Tab, _) => self.toggle_preview_tab(),
            (KeyCode::Char('g'), _) => self.move_to_top(),
            (KeyCode::Char('G'), _) => self.move_to_bottom(),
            (KeyCode::Char('`'), _) => self.cycle_mode(),
            (KeyCode::Char('w'), _) => self.open_warnings(),
            (KeyCode::Char('s'), _) => self.open_status_picker(),
            (KeyCode::Char('p'), _) => self.open_provenance_editor(),
            (KeyCode::Char('r'), _) if self.preview_tab == PreviewTab::Relations => {
                self.open_link_editor();
            }
            #[cfg(feature = "agent")]
            (KeyCode::Char('a'), _) => {
                if let Some(doc) = self.selected_doc_meta() {
                    let doc_type_str = doc.doc_type.to_string();
                    let doc_path = doc.path.clone();
                    let doc_title = doc.title.clone();

                    let has_children = config.rules.iter().any(|rule| {
                        matches!(rule, crate::engine::config::ValidationRule::ParentChild { parent, .. } if parent == &doc_type_str)
                    });

                    let mut actions =
                        vec!["Expand document".to_string(), "Custom prompt".to_string()];
                    if has_children {
                        actions.push("Create children".to_string());
                    }

                    self.agent_dialog = crate::tui::state::forms::AgentDialog {
                        active: true,
                        selected_index: 0,
                        actions,
                        doc_path,
                        doc_title,
                        text_input: None,
                    };
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sequencing::Scope;
    use crate::engine::store::Store;
    use crate::tui::state::App;
    use std::collections::BTreeMap;
    use std::path::Path as StdPath;
    use tempfile::TempDir;

    fn write(root: &StdPath, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// Build a small docs tree: RFC-1 -> STORY-1 -> ITERATION-1.
    fn make_app() -> (TempDir, App) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("docs/rfcs")).unwrap();
        std::fs::create_dir_all(root.join("docs/stories")).unwrap();
        std::fs::create_dir_all(root.join("docs/iterations")).unwrap();

        write(
            root,
            "docs/rfcs/RFC-1-root.md",
            "---\ntitle: \"Root\"\ntype: rfc\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\n---\n",
        );
        write(
            root,
            "docs/stories/STORY-1-feat.md",
            "---\ntitle: \"Feat\"\ntype: story\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\npriority: should\nrelated:\n- implements: RFC-1\n---\n",
        );
        write(
            root,
            "docs/iterations/ITERATION-1-impl.md",
            "---\ntitle: \"Impl\"\ntype: iteration\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\npriority: should\nrelated:\n- implements: STORY-1\n---\n",
        );

        let cfg = Config::default();
        let store = Store::load(root, &cfg).unwrap();
        let picker = ratatui_image::picker::Picker::halfblocks();
        let fs = Box::new(crate::engine::fs::RealFileSystem);
        let mut app = App::new(store, &cfg, picker, fs);
        app.view_mode = ViewMode::Sequencing;
        (tmp, app)
    }

    fn doc_paths(root: &StdPath) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for sub in ["docs/rfcs", "docs/stories", "docs/iterations"] {
            for entry in std::fs::read_dir(root.join(sub)).unwrap() {
                out.push(entry.unwrap().path());
            }
        }
        out.sort();
        out
    }

    fn snapshot_disk(root: &StdPath) -> BTreeMap<std::path::PathBuf, (std::time::SystemTime, Vec<u8>)> {
        let mut out = BTreeMap::new();
        for p in doc_paths(root) {
            let meta = std::fs::metadata(&p).unwrap();
            let mtime = meta.modified().unwrap();
            let bytes = std::fs::read(&p).unwrap();
            out.insert(p, (mtime, bytes));
        }
        out
    }

    fn type_chars(app: &mut App, s: &str) {
        for c in s.chars() {
            app.handle_sequencing_key(KeyCode::Char(c));
        }
    }

    /// AC6: pressing `s`, typing a story id, and pressing Enter sets `Under`.
    #[test]
    fn s_key_sets_scope_under_with_typed_story_id() {
        let (_tmp, mut app) = make_app();

        app.handle_sequencing_key(KeyCode::Char('s'));
        type_chars(&mut app, "STORY-1");
        app.handle_sequencing_key(KeyCode::Enter);

        assert_eq!(app.sequencing.scope, Scope::Under("STORY-1".to_string()));
        assert!(app.sequencing.scope_input.is_none());
        assert!(app.sequencing.error.is_none());
    }

    /// AC6: typing an iteration id is rejected; previous scope and membership
    /// are preserved and `error` is populated.
    #[test]
    fn iteration_id_rejected_preserves_prior_scope_and_sets_error() {
        let (_tmp, mut app) = make_app();
        // Establish a prior scope first.
        app.handle_sequencing_key(KeyCode::Char('s'));
        type_chars(&mut app, "STORY-1");
        app.handle_sequencing_key(KeyCode::Enter);
        let prior_scope = app.sequencing.scope.clone();
        let prior_membership = app.sequencing.in_scope.clone();

        // Now try to scope to an iteration.
        app.handle_sequencing_key(KeyCode::Char('s'));
        type_chars(&mut app, "ITERATION-1");
        app.handle_sequencing_key(KeyCode::Enter);

        assert_eq!(app.sequencing.scope, prior_scope, "scope must not change");
        assert_eq!(
            app.sequencing.in_scope, prior_membership,
            "membership must not change"
        );
        let err = app.sequencing.error.as_ref().expect("expected error");
        assert!(
            err.to_lowercase().contains("iteration"),
            "error should mention iteration: {:?}",
            err
        );
    }

    /// AC6: `f` opens the After-scope input.
    #[test]
    fn f_key_sets_scope_after_with_typed_id() {
        let (_tmp, mut app) = make_app();

        app.handle_sequencing_key(KeyCode::Char('f'));
        type_chars(&mut app, "RFC-1");
        app.handle_sequencing_key(KeyCode::Enter);

        assert_eq!(app.sequencing.scope, Scope::After("RFC-1".to_string()));
    }

    /// AC6: `c` clears the active scope back to `All`.
    #[test]
    fn c_key_clears_scope() {
        let (_tmp, mut app) = make_app();
        app.handle_sequencing_key(KeyCode::Char('s'));
        type_chars(&mut app, "STORY-1");
        app.handle_sequencing_key(KeyCode::Enter);
        assert_ne!(app.sequencing.scope, Scope::All);

        app.handle_sequencing_key(KeyCode::Char('c'));

        assert_eq!(app.sequencing.scope, Scope::All);
    }

    /// AC6: `Esc` cancels an in-progress scope-input without applying it.
    #[test]
    fn esc_cancels_scope_input_buffer() {
        let (_tmp, mut app) = make_app();
        let prior_scope = app.sequencing.scope.clone();

        app.handle_sequencing_key(KeyCode::Char('s'));
        type_chars(&mut app, "STORY-1");
        app.handle_sequencing_key(KeyCode::Esc);

        assert!(app.sequencing.scope_input.is_none());
        assert_eq!(app.sequencing.scope, prior_scope);
    }

    /// AC7: edit gestures (`a`, `d`, priority keys `1`/`2`/`3`) MUST NOT
    /// mutate any document on disk. They surface an info message instead.
    #[test]
    fn edit_gestures_do_not_mutate_docs_on_disk() {
        let (tmp, mut app) = make_app();
        let root = tmp.path().to_path_buf();
        let before = snapshot_disk(&root);

        for c in ['a', 'd', '1', '2', '3'] {
            app.handle_sequencing_key(KeyCode::Char(c));
        }

        let after = snapshot_disk(&root);
        assert_eq!(
            before, after,
            "edit gestures must not modify any doc on disk"
        );
        let info = app.sequencing.info.as_ref().expect("expected info msg");
        assert!(
            info.to_lowercase().contains("read-only"),
            "info should mention read-only: {:?}",
            info
        );
    }
}
