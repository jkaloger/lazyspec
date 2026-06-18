use crate::engine::config::Config;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::Path;

#[cfg(feature = "agent")]
use crate::tui::agent::AgentStatus;

use crate::tui::state::{App, FilterField, PreviewTab, ViewMode};

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
                if !self.rel_types.is_empty() {
                    self.link_editor.rel_type_index =
                        (self.link_editor.rel_type_index + 1) % self.rel_types.len();
                }
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

    /// Open the template-driven agent dialog for the selected doc (RFC-046 slice
    /// 4). Lists one entry per prompt template resolved for the doc's type, plus a
    /// Custom entry. Opens nothing when the type exposes no agents (AC7).
    #[cfg(feature = "agent")]
    fn open_agent_dialog(&mut self, config: &Config) {
        use crate::tui::state::forms::{AgentAction, AgentDialog};

        let doc = match self.selected_doc_meta() {
            Some(d) => d,
            None => return,
        };
        let doc_type = doc.doc_type.as_str().to_string();
        let doc_path = doc.path.clone();
        let doc_title = doc.title.clone();

        let type_agents = config
            .type_by_name(&doc_type)
            .map(|t| t.agents.clone())
            .unwrap_or_default();

        let loaded_names: Vec<String> = self.agent_prompts.iter().map(|p| p.name.clone()).collect();
        let resolved = crate::engine::agent::resolve_agent_actions(&type_agents, &loaded_names);

        // AC5: interactive templates are offered ONLY when `[agents] interactive`
        // is configured (zero-defaults). Headless templates are always included.
        let interactive_available = config.agents.interactive.is_some();

        let mut actions: Vec<AgentAction> = resolved
            .actions
            .iter()
            .filter_map(|name| {
                self.agent_prompts
                    .iter()
                    .find(|p| &p.name == name)
                    .cloned()
                    .map(AgentAction::Template)
            })
            .filter(|action| match action {
                AgentAction::Template(p) => {
                    interactive_available || p.mode != crate::engine::prompt::RunMode::Interactive
                }
                AgentAction::Custom => true,
            })
            .collect();

        if !type_agents.is_empty() {
            actions.push(AgentAction::Custom);
        }

        // AC7: no resolvable actions (type exposes no agents) -> open nothing.
        if actions.is_empty() {
            return;
        }

        self.agent_dialog = AgentDialog {
            active: true,
            selected_index: 0,
            actions,
            missing: resolved.missing,
            doc_path,
            doc_title,
            text_input: None,
        };
    }

    #[cfg(feature = "agent")]
    fn handle_agent_dialog_key(&mut self, code: KeyCode, config: &Config) {
        use crate::tui::state::forms::AgentAction;

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
                let action = match self
                    .agent_dialog
                    .actions
                    .get(self.agent_dialog.selected_index)
                    .cloned()
                {
                    Some(a) => a,
                    None => return,
                };

                match action {
                    AgentAction::Custom => {
                        // Full Custom spawn is the next unit; just open the input.
                        self.agent_dialog.text_input = Some(String::new());
                    }
                    AgentAction::Template(prompt) => {
                        use crate::engine::prompt::RunMode;
                        match prompt.mode {
                            RunMode::Headless => {
                                // Existing slice-4 path (AgentSpawner/AgentRunner, records AgentRecord).
                                let doc_path = self.agent_dialog.doc_path.clone();
                                let doc_title = self.agent_dialog.doc_title.clone();
                                self.agent_dialog.active = false;

                                let doc = match self.store.get(&doc_path).cloned() {
                                    Some(d) => d,
                                    None => return,
                                };

                                let ctx = match crate::engine::prompt::build_render_context(
                                    &self.store,
                                    config,
                                    &doc,
                                    &*self.fs,
                                ) {
                                    Ok(c) => c,
                                    Err(_) => return,
                                };

                                let rendered = match crate::engine::prompt::render(&prompt, &ctx) {
                                    Ok(r) => r,
                                    Err(_) => return,
                                };

                                let full_path = self.store.root.join(&doc_path);
                                let _ = self.agent_spawner.spawn(
                                    &rendered,
                                    prompt.allowed_tools.as_deref(),
                                    &full_path,
                                    &doc_title,
                                    &prompt.name,
                                );
                            }
                            RunMode::Interactive => {
                                // Render the tmpl body for the doc (slice-2 render entrypoint).
                                let doc_path = self.agent_dialog.doc_path.clone();
                                let doc = match self.store.get(&doc_path).cloned() {
                                    Some(d) => d,
                                    None => return,
                                };
                                let ctx = match crate::engine::prompt::build_render_context(
                                    &self.store,
                                    config,
                                    &doc,
                                    &*self.fs,
                                ) {
                                    Ok(c) => c,
                                    Err(_) => return,
                                };
                                let rendered = match crate::engine::prompt::render(&prompt, &ctx) {
                                    Ok(r) => r,
                                    Err(_) => return,
                                };
                                self.agent_dialog.active = false;
                                self.interactive_request =
                                    Some(crate::tui::state::forms::InteractiveRequest {
                                        cmd: config.agents.interactive.clone().unwrap(),
                                        prompt: rendered,
                                        doc_path: self.store.root.join(&doc_path),
                                    });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
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
                            None,
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
            KeyCode::Char('r') => {
                self.open_link_editor();
            }
            KeyCode::Char('p') => {
                self.open_provenance_editor();
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
            (KeyCode::Char('x'), _) => {
                self.wrap_mode = !self.wrap_mode;
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
            (KeyCode::Char('r'), _) => {
                self.open_link_editor();
            }
            #[cfg(feature = "agent")]
            (KeyCode::Char('a'), _) => {
                self.open_agent_dialog(config);
            }
            _ => {}
        }
    }
}
