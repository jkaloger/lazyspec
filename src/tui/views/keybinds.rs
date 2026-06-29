//! Single source of truth for the TUI keybind registry.
//!
//! Each [`KeyContext`] corresponds to one active handler in `handle_key`'s
//! precedence ladder (see `keys.rs`). [`keybinds_for`] returns the honest set of
//! keys that handler acts on -- dialogs do NOT carry the global chrome
//! (`/`, `5`, `q`, ...). The help overlay renders from this registry, and a
//! parity test asserts the registry matches the handlers exactly.

use crossterm::event::KeyCode;

/// One active handler in `handle_key`'s precedence ladder (`keys.rs:19-66`).
///
/// The ladder order here mirrors the precedence in `handle_key`, plus the
/// `handle_normal_key` view-mode dispatch (`keys.rs:1003-1009`) and the nested
/// settings sub-handlers (`handle_settings_key`, `keys.rs:735-993`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyContext {
    GhConflict,
    Warnings,
    CreateForm,
    DeleteConfirm,
    OverrideKeyPrompt,
    SettingsDeleteConfirm,
    SettingsImpact,
    StatusPicker,
    LinkEditor,
    ProvenanceEditor,
    #[cfg(feature = "agent")]
    AgentDialog,
    #[cfg(feature = "agent")]
    AgentTextInput,
    Search,
    Fullscreen,
    Types,
    Filters,
    Graph,
    #[cfg(feature = "agent")]
    Agents,
    Settings,
    SettingsEditing,
    SettingsQuitPrompt,
    SettingsZoneEditor,
    SettingsVariantPicker,
    SettingsScaffoldOffer,
}

/// A single physical key chord: a [`KeyCode`] plus whether Ctrl is held. The
/// display string on a [`Keybind`] is human-facing and ambiguous; this is the
/// machine-readable form the parity test compares against the handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    pub code: KeyCode,
    pub ctrl: bool,
}

/// A plain (no-modifier) chord.
const fn k(code: KeyCode) -> KeyChord {
    KeyChord { code, ctrl: false }
}

/// A Ctrl-modified chord.
const fn ctrl(code: KeyCode) -> KeyChord {
    KeyChord { code, ctrl: true }
}

/// One distinct action the user can take in a context. `keys` is the display
/// string (e.g. `"j/k"`); `chords` is every physical chord that triggers the
/// action. `char_catchall` marks a `Char(c) =>` type-text arm: such binds carry
/// no explicit chords because the catch-all is represented by the flag.
/// `any_key` marks an "any other key" arm (e.g. the scaffold offer's dismiss):
/// every press the context doesn't bind explicitly triggers this action, char or
/// not. It is broader than `char_catchall` (which only swallows printable chars).
#[derive(Debug, Clone, Copy)]
pub struct Keybind {
    pub keys: &'static str,
    pub desc: &'static str,
    pub chords: &'static [KeyChord],
    pub char_catchall: bool,
    pub any_key: bool,
}

/// A titled cluster of related binds, for sectioned rendering in the popup.
#[derive(Debug, Clone)]
pub struct KeybindGroup {
    pub title: &'static str,
    pub binds: Vec<Keybind>,
}

/// Build a [`Keybind`]. The chord list is wrapped in a `const {}` block so the
/// slice is promoted to `'static` (inline `&[...]` literals inside the `vec!`
/// arguments below would otherwise be temporaries).
macro_rules! bind {
    ($keys:literal, $desc:literal, [$($chord:expr),* $(,)?]) => {
        Keybind {
            keys: $keys,
            desc: $desc,
            chords: const { &[$($chord),*] },
            char_catchall: false,
            any_key: false,
        }
    };
}

/// A type-text catch-all bind. Represents the `Char(c) =>` arm; carries no
/// explicit chords (the flag is the contract).
const fn type_text() -> Keybind {
    Keybind {
        keys: "<any>",
        desc: "Type text",
        chords: &[],
        char_catchall: true,
        any_key: false,
    }
}

/// An "any other key" bind. Represents an arm where every press not bound
/// explicitly triggers `desc` (the scaffold offer's dismiss-on-any-key). Carries
/// no explicit chords; the flag is the contract. Its display token is bracketed
/// so `display_matches_chords` does not read it as a single-char chord.
const fn any_key(desc: &'static str) -> Keybind {
    Keybind {
        keys: "<any other>",
        desc,
        chords: &[],
        char_catchall: false,
        any_key: true,
    }
}

// NB on `?`: in Fullscreen / Graph / Settings nav / Agents, `keys.rs` does NOT
// yet handle `?` -- task T4 wires it. Those `?` "Toggle help" binds live in the
// registry now so parity holds the moment T4 lands.

/// The honest per-context keybind list: only keys the handler actually acts on.
pub fn keybinds_for(ctx: KeyContext) -> Vec<KeybindGroup> {
    match ctx {
        // keys.rs:19 -- only Esc closes.
        KeyContext::GhConflict => vec![KeybindGroup {
            title: "Conflict",
            binds: vec![bind!("Esc", "Close", [k(KeyCode::Esc)])],
        }],

        // handle_warnings_key, keys.rs:192.
        KeyContext::Warnings => vec![KeybindGroup {
            title: "Warnings",
            binds: vec![
                bind!(
                    "Esc/w/q",
                    "Close",
                    [
                        k(KeyCode::Esc),
                        k(KeyCode::Char('w')),
                        k(KeyCode::Char('q'))
                    ]
                ),
                bind!("f", "Fix", [k(KeyCode::Char('f'))]),
                bind!(
                    "j/k",
                    "Navigate",
                    [
                        k(KeyCode::Char('j')),
                        k(KeyCode::Down),
                        k(KeyCode::Char('k')),
                        k(KeyCode::Up)
                    ]
                ),
            ],
        }],

        // handle_create_form_key, keys.rs:69. Non-loading set (loading allows
        // only Esc, handled at runtime).
        KeyContext::CreateForm => vec![KeybindGroup {
            title: "New document",
            binds: vec![
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
                bind!("Enter", "Submit", [k(KeyCode::Enter)]),
                bind!("Tab", "Next field", [k(KeyCode::Tab)]),
                bind!("BackTab", "Previous field", [k(KeyCode::BackTab)]),
                bind!("Backspace", "Delete char", [k(KeyCode::Backspace)]),
                type_text(),
            ],
        }],

        // handle_delete_confirm_key, keys.rs:89.
        KeyContext::DeleteConfirm => vec![KeybindGroup {
            title: "Delete document",
            binds: vec![
                bind!("Enter", "Confirm", [k(KeyCode::Enter)]),
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
            ],
        }],

        // handle_override_key_prompt_key, keys.rs:115.
        KeyContext::OverrideKeyPrompt => vec![KeybindGroup {
            title: "Certification override key",
            binds: vec![
                bind!("Enter", "Confirm", [k(KeyCode::Enter)]),
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
                bind!("Backspace", "Delete char", [k(KeyCode::Backspace)]),
                type_text(),
            ],
        }],

        // handle_settings_delete_confirm_key, keys.rs:99.
        KeyContext::SettingsDeleteConfirm => vec![KeybindGroup {
            title: "Delete entry",
            binds: vec![
                bind!("Enter", "Confirm", [k(KeyCode::Enter)]),
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
            ],
        }],

        // handle_settings_impact_key, keys.rs:107.
        KeyContext::SettingsImpact => vec![KeybindGroup {
            title: "Document impact",
            binds: vec![
                bind!(
                    "Enter/y",
                    "Confirm",
                    [k(KeyCode::Enter), k(KeyCode::Char('y'))]
                ),
                bind!("Esc/n", "Cancel", [k(KeyCode::Esc), k(KeyCode::Char('n'))]),
            ],
        }],

        // handle_status_picker_key, keys.rs:125.
        KeyContext::StatusPicker => vec![KeybindGroup {
            title: "Status",
            binds: vec![
                bind!(
                    "j/k",
                    "Navigate",
                    [
                        k(KeyCode::Char('j')),
                        k(KeyCode::Down),
                        k(KeyCode::Char('k')),
                        k(KeyCode::Up)
                    ]
                ),
                bind!("Enter", "Select", [k(KeyCode::Enter)]),
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
            ],
        }],

        // handle_link_editor_key, keys.rs.
        KeyContext::LinkEditor => vec![KeybindGroup {
            title: "Relation",
            binds: vec![
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
                bind!(
                    "←/→",
                    "Cycle relation type",
                    [
                        k(KeyCode::Left),
                        k(KeyCode::Right),
                        k(KeyCode::Tab),
                        k(KeyCode::BackTab)
                    ]
                ),
                bind!(
                    "↑/↓",
                    "Navigate results",
                    [k(KeyCode::Up), k(KeyCode::Down)]
                ),
                bind!("Enter", "Confirm", [k(KeyCode::Enter)]),
                bind!("Backspace", "Delete char", [k(KeyCode::Backspace)]),
                type_text(),
            ],
        }],

        // handle_provenance_editor_key, keys.rs:180.
        KeyContext::ProvenanceEditor => vec![KeybindGroup {
            title: "Provenance",
            binds: vec![
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
                bind!("Enter", "Confirm", [k(KeyCode::Enter)]),
                bind!("Backspace", "Delete char", [k(KeyCode::Backspace)]),
                type_text(),
            ],
        }],

        // handle_agent_dialog_key, keys.rs:269.
        #[cfg(feature = "agent")]
        KeyContext::AgentDialog => vec![KeybindGroup {
            title: "Agent",
            binds: vec![
                bind!("Esc", "Close", [k(KeyCode::Esc)]),
                bind!("Up/Down", "Navigate", [k(KeyCode::Up), k(KeyCode::Down)]),
                bind!("Enter", "Select", [k(KeyCode::Enter)]),
            ],
        }],

        // handle_agent_text_input_key, keys.rs:387.
        #[cfg(feature = "agent")]
        KeyContext::AgentTextInput => vec![KeybindGroup {
            title: "Agent · Custom prompt",
            binds: vec![
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
                bind!("Enter", "Submit", [k(KeyCode::Enter)]),
                bind!("Backspace", "Delete char", [k(KeyCode::Backspace)]),
                type_text(),
            ],
        }],

        // handle_search_key, keys.rs:431.
        KeyContext::Search => vec![KeybindGroup {
            title: "Search",
            binds: vec![
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
                bind!("Enter", "Open result", [k(KeyCode::Enter)]),
                bind!("Backspace", "Delete char", [k(KeyCode::Backspace)]),
                bind!(
                    "Up/Ctrl-k",
                    "Previous result",
                    [k(KeyCode::Up), ctrl(KeyCode::Char('k'))]
                ),
                bind!(
                    "Down/Ctrl-j",
                    "Next result",
                    [k(KeyCode::Down), ctrl(KeyCode::Char('j'))]
                ),
                type_text(),
            ],
        }],

        // handle_fullscreen_key, keys.rs:455. T4 wires `?` here.
        KeyContext::Fullscreen => vec![KeybindGroup {
            title: "Fullscreen",
            binds: vec![
                bind!("Esc/q", "Exit", [k(KeyCode::Esc), k(KeyCode::Char('q'))]),
                bind!(
                    "j/k",
                    "Scroll",
                    [
                        k(KeyCode::Char('j')),
                        k(KeyCode::Down),
                        k(KeyCode::Char('k')),
                        k(KeyCode::Up)
                    ]
                ),
                bind!("g", "Top", [k(KeyCode::Char('g'))]),
                bind!("G", "Bottom", [k(KeyCode::Char('G'))]),
                bind!("Ctrl-d", "Half page down", [ctrl(KeyCode::Char('d'))]),
                bind!("Ctrl-u", "Half page up", [ctrl(KeyCode::Char('u'))]),
                bind!("?", "Toggle help", [k(KeyCode::Char('?'))]),
            ],
        }],

        // handle_normal_key Types default, keys.rs:996/1012.
        KeyContext::Types => vec![
            KeybindGroup {
                title: "Navigation",
                binds: vec![
                    bind!(
                        "j/k",
                        "Navigate",
                        [
                            k(KeyCode::Char('j')),
                            k(KeyCode::Down),
                            k(KeyCode::Char('k')),
                            k(KeyCode::Up)
                        ]
                    ),
                    bind!(
                        "h/l",
                        "Switch type",
                        [
                            k(KeyCode::Char('h')),
                            k(KeyCode::Left),
                            k(KeyCode::Char('l')),
                            k(KeyCode::Right)
                        ]
                    ),
                    bind!("g", "Top", [k(KeyCode::Char('g'))]),
                    bind!("G", "Bottom", [k(KeyCode::Char('G'))]),
                    bind!("Ctrl-d", "Half page down", [ctrl(KeyCode::Char('d'))]),
                    bind!("Ctrl-u", "Half page up", [ctrl(KeyCode::Char('u'))]),
                    bind!("Space", "Expand/collapse", [k(KeyCode::Char(' '))]),
                    bind!("Tab", "Preview tab", [k(KeyCode::Tab)]),
                    bind!("Enter", "Open/follow relation", [k(KeyCode::Enter)]),
                ],
            },
            KeybindGroup {
                title: "Document",
                binds: vec![
                    bind!("n", "New", [k(KeyCode::Char('n'))]),
                    bind!("e", "Edit", [k(KeyCode::Char('e'))]),
                    bind!("d", "Delete", [k(KeyCode::Char('d'))]),
                    bind!("s", "Status", [k(KeyCode::Char('s'))]),
                    bind!("r", "Relation", [k(KeyCode::Char('r'))]),
                    bind!("p", "Provenance", [k(KeyCode::Char('p'))]),
                    bind!("R", "Reload config", [k(KeyCode::Char('R'))]),
                    #[cfg(feature = "agent")]
                    bind!("a", "Agent", [k(KeyCode::Char('a'))]),
                ],
            },
            KeybindGroup {
                title: "View",
                binds: vec![
                    bind!("x", "Toggle wrap", [k(KeyCode::Char('x'))]),
                    bind!("/", "Search", [k(KeyCode::Char('/'))]),
                    bind!("w", "Warnings", [k(KeyCode::Char('w'))]),
                    bind!("`", "Cycle mode", [k(KeyCode::Char('`'))]),
                    bind!("5", "Settings", [k(KeyCode::Char('5'))]),
                    bind!("?", "Toggle help", [k(KeyCode::Char('?'))]),
                    bind!(
                        "q/Ctrl-c",
                        "Quit",
                        [k(KeyCode::Char('q')), ctrl(KeyCode::Char('c'))]
                    ),
                ],
            },
        ],

        // handle_filters_key, keys.rs:531.
        KeyContext::Filters => vec![
            KeybindGroup {
                title: "Navigation",
                binds: vec![
                    bind!(
                        "j/k",
                        "Navigate",
                        [
                            k(KeyCode::Char('j')),
                            k(KeyCode::Down),
                            k(KeyCode::Char('k')),
                            k(KeyCode::Up)
                        ]
                    ),
                    bind!("g", "Top", [k(KeyCode::Char('g'))]),
                    bind!("G", "Bottom", [k(KeyCode::Char('G'))]),
                    bind!("Ctrl-d", "Half page down", [ctrl(KeyCode::Char('d'))]),
                    bind!("Ctrl-u", "Half page up", [ctrl(KeyCode::Char('u'))]),
                ],
            },
            KeybindGroup {
                title: "Filters",
                binds: vec![
                    bind!(
                        "Tab/BackTab",
                        "Focus field",
                        [k(KeyCode::Tab), k(KeyCode::BackTab)]
                    ),
                    bind!(
                        "h/l",
                        "Cycle value",
                        [
                            k(KeyCode::Char('h')),
                            k(KeyCode::Left),
                            k(KeyCode::Char('l')),
                            k(KeyCode::Right)
                        ]
                    ),
                    bind!("Enter", "Clear/open/follow", [k(KeyCode::Enter)]),
                ],
            },
            KeybindGroup {
                title: "Document",
                binds: vec![
                    bind!("e", "Edit", [k(KeyCode::Char('e'))]),
                    bind!("s", "Status", [k(KeyCode::Char('s'))]),
                    bind!("r", "Relation", [k(KeyCode::Char('r'))]),
                    bind!("p", "Provenance", [k(KeyCode::Char('p'))]),
                ],
            },
            KeybindGroup {
                title: "View",
                binds: vec![
                    bind!("/", "Search", [k(KeyCode::Char('/'))]),
                    bind!("w", "Warnings", [k(KeyCode::Char('w'))]),
                    bind!("`", "Cycle mode", [k(KeyCode::Char('`'))]),
                    bind!("5", "Settings", [k(KeyCode::Char('5'))]),
                    bind!("?", "Toggle help", [k(KeyCode::Char('?'))]),
                    bind!("q", "Quit", [k(KeyCode::Char('q'))]),
                ],
            },
        ],

        // handle_graph_key, keys.rs:632. T4 wires `?` here.
        KeyContext::Graph => vec![KeybindGroup {
            title: "Graph",
            binds: vec![
                bind!(
                    "j/k",
                    "Navigate",
                    [
                        k(KeyCode::Char('j')),
                        k(KeyCode::Down),
                        k(KeyCode::Char('k')),
                        k(KeyCode::Up)
                    ]
                ),
                bind!(
                    "h/l",
                    "Pivot anchor (type / tag)",
                    [
                        k(KeyCode::Char('h')),
                        k(KeyCode::Left),
                        k(KeyCode::Char('l')),
                        k(KeyCode::Right)
                    ]
                ),
                bind!("Ctrl-d", "Half page down", [ctrl(KeyCode::Char('d'))]),
                bind!("Ctrl-u", "Half page up", [ctrl(KeyCode::Char('u'))]),
                bind!("o", "Cycle sort column", [k(KeyCode::Char('o'))]),
                bind!("O", "Reverse sort", [k(KeyCode::Char('O'))]),
                bind!("g", "Top", [k(KeyCode::Char('g'))]),
                bind!("G", "Bottom", [k(KeyCode::Char('G'))]),
                bind!("Enter", "Open", [k(KeyCode::Enter)]),
                bind!("e", "Edit", [k(KeyCode::Char('e'))]),
                bind!("`", "Cycle mode", [k(KeyCode::Char('`'))]),
                bind!("5", "Settings", [k(KeyCode::Char('5'))]),
                bind!("?", "Toggle help", [k(KeyCode::Char('?'))]),
                bind!("q", "Quit", [k(KeyCode::Char('q'))]),
            ],
        }],

        // handle_agents_key, keys.rs:476. T4 wires `?` here.
        #[cfg(feature = "agent")]
        KeyContext::Agents => vec![KeybindGroup {
            title: "Agents",
            binds: vec![
                bind!(
                    "j/k",
                    "Navigate",
                    [
                        k(KeyCode::Char('j')),
                        k(KeyCode::Down),
                        k(KeyCode::Char('k')),
                        k(KeyCode::Up)
                    ]
                ),
                bind!("Ctrl-d", "Half page down", [ctrl(KeyCode::Char('d'))]),
                bind!("Ctrl-u", "Half page up", [ctrl(KeyCode::Char('u'))]),
                bind!("e", "Edit", [k(KeyCode::Char('e'))]),
                bind!("r", "Resume", [k(KeyCode::Char('r'))]),
                bind!("`", "Cycle mode", [k(KeyCode::Char('`'))]),
                bind!("5", "Settings", [k(KeyCode::Char('5'))]),
                bind!("?", "Toggle help", [k(KeyCode::Char('?'))]),
                bind!("q", "Quit", [k(KeyCode::Char('q'))]),
            ],
        }],

        // handle_settings_key nav, keys.rs:894. `5` is a no-op (not listed).
        // T4 wires `?` here.
        KeyContext::Settings => vec![
            KeybindGroup {
                title: "Navigation",
                binds: vec![
                    bind!(
                        "j/k",
                        "Field/entry",
                        [
                            k(KeyCode::Char('j')),
                            k(KeyCode::Down),
                            k(KeyCode::Char('k')),
                            k(KeyCode::Up)
                        ]
                    ),
                    bind!(
                        "h/l",
                        "Category",
                        [
                            k(KeyCode::Char('h')),
                            k(KeyCode::Left),
                            k(KeyCode::Char('l')),
                            k(KeyCode::Right)
                        ]
                    ),
                    bind!("Enter", "Edit/drill", [k(KeyCode::Enter)]),
                    bind!("Esc", "Back/quit", [k(KeyCode::Esc)]),
                ],
            },
            KeybindGroup {
                title: "Entries",
                binds: vec![
                    bind!("n", "New entry", [k(KeyCode::Char('n'))]),
                    bind!("d", "Delete entry", [k(KeyCode::Char('d'))]),
                ],
            },
            KeybindGroup {
                title: "Actions",
                binds: vec![
                    bind!(
                        "w/Ctrl-s",
                        "Save",
                        [k(KeyCode::Char('w')), ctrl(KeyCode::Char('s'))]
                    ),
                    bind!("`", "Cycle mode", [k(KeyCode::Char('`'))]),
                    bind!("?", "Toggle help", [k(KeyCode::Char('?'))]),
                    bind!("q", "Quit", [k(KeyCode::Char('q'))]),
                ],
            },
        ],

        // handle_settings_key editing branch, keys.rs:776.
        KeyContext::SettingsEditing => vec![KeybindGroup {
            title: "Settings · Edit field",
            binds: vec![
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
                bind!("Enter", "Confirm", [k(KeyCode::Enter)]),
                bind!("Backspace", "Delete char", [k(KeyCode::Backspace)]),
                type_text(),
            ],
        }],

        // handle_settings_key quit-prompt branch, keys.rs:744.
        KeyContext::SettingsQuitPrompt => vec![KeybindGroup {
            title: "Settings · Unsaved changes",
            binds: vec![
                bind!("s", "Save", [k(KeyCode::Char('s'))]),
                bind!("d", "Discard", [k(KeyCode::Char('d'))]),
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
            ],
        }],

        // handle_settings_key zone-editor branch, keys.rs:795.
        KeyContext::SettingsZoneEditor => vec![KeybindGroup {
            title: "Settings · Zone order",
            binds: vec![
                bind!("Tab", "Switch pane", [k(KeyCode::Tab)]),
                bind!(
                    "j/k",
                    "Navigate",
                    [
                        k(KeyCode::Char('j')),
                        k(KeyCode::Down),
                        k(KeyCode::Char('k')),
                        k(KeyCode::Up)
                    ]
                ),
                bind!(
                    "Space/Enter",
                    "Add/remove",
                    [k(KeyCode::Char(' ')), k(KeyCode::Enter)]
                ),
                bind!("K", "Move up", [k(KeyCode::Char('K'))]),
                bind!("J", "Move down", [k(KeyCode::Char('J'))]),
                bind!("c", "Commit", [k(KeyCode::Char('c'))]),
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
            ],
        }],

        // handle_settings_key variant-picker branch, keys.rs:841.
        KeyContext::SettingsVariantPicker => vec![KeybindGroup {
            title: "Settings · Choose variant",
            binds: vec![
                bind!(
                    "j/k",
                    "Navigate",
                    [
                        k(KeyCode::Char('j')),
                        k(KeyCode::Down),
                        k(KeyCode::Char('k')),
                        k(KeyCode::Up)
                    ]
                ),
                bind!("Enter", "Select", [k(KeyCode::Enter)]),
                bind!("Esc", "Cancel", [k(KeyCode::Esc)]),
            ],
        }],

        // handle_settings_key scaffold-offer branch, keys.rs:903. `g` jumps to the
        // required field; any other key dismisses the offer (and falls through).
        KeyContext::SettingsScaffoldOffer => vec![KeybindGroup {
            title: "Settings · Dependency scaffold",
            binds: vec![
                bind!("g", "Jump to field", [k(KeyCode::Char('g'))]),
                any_key("Dismiss"),
            ],
        }],
    }
}

/// Popup title for a context.
pub fn context_label(ctx: KeyContext) -> &'static str {
    match ctx {
        KeyContext::GhConflict => "GitHub conflict",
        KeyContext::Warnings => "Warnings",
        KeyContext::CreateForm => "New document",
        KeyContext::DeleteConfirm => "Delete document",
        KeyContext::OverrideKeyPrompt => "Certification override",
        KeyContext::SettingsDeleteConfirm => "Settings · Delete entry",
        KeyContext::SettingsImpact => "Settings · Document impact",
        KeyContext::StatusPicker => "Status",
        KeyContext::LinkEditor => "Relation",
        KeyContext::ProvenanceEditor => "Provenance",
        #[cfg(feature = "agent")]
        KeyContext::AgentDialog => "Agent",
        #[cfg(feature = "agent")]
        KeyContext::AgentTextInput => "Agent · Custom prompt",
        KeyContext::Search => "Search",
        KeyContext::Fullscreen => "Fullscreen",
        KeyContext::Types => "Types",
        KeyContext::Filters => "Filters",
        KeyContext::Graph => "Graph",
        #[cfg(feature = "agent")]
        KeyContext::Agents => "Agents",
        KeyContext::Settings => "Settings",
        KeyContext::SettingsEditing => "Settings · Edit field",
        KeyContext::SettingsQuitPrompt => "Settings · Unsaved changes",
        KeyContext::SettingsZoneEditor => "Settings · Zone order",
        KeyContext::SettingsVariantPicker => "Settings · Choose variant",
        KeyContext::SettingsScaffoldOffer => "Settings · Dependency scaffold",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every KeyContext variant, for exhaustive iteration in tests.
    const ALL_CONTEXTS: &[KeyContext] = &[
        KeyContext::GhConflict,
        KeyContext::Warnings,
        KeyContext::CreateForm,
        KeyContext::DeleteConfirm,
        KeyContext::OverrideKeyPrompt,
        KeyContext::SettingsDeleteConfirm,
        KeyContext::SettingsImpact,
        KeyContext::StatusPicker,
        KeyContext::LinkEditor,
        KeyContext::ProvenanceEditor,
        #[cfg(feature = "agent")]
        KeyContext::AgentDialog,
        #[cfg(feature = "agent")]
        KeyContext::AgentTextInput,
        KeyContext::Search,
        KeyContext::Fullscreen,
        KeyContext::Types,
        KeyContext::Filters,
        KeyContext::Graph,
        #[cfg(feature = "agent")]
        KeyContext::Agents,
        KeyContext::Settings,
        KeyContext::SettingsEditing,
        KeyContext::SettingsQuitPrompt,
        KeyContext::SettingsZoneEditor,
        KeyContext::SettingsVariantPicker,
        KeyContext::SettingsScaffoldOffer,
    ];

    fn binds(ctx: KeyContext) -> Vec<Keybind> {
        keybinds_for(ctx)
            .into_iter()
            .flat_map(|g| g.binds)
            .collect()
    }

    fn has_desc_containing(ctx: KeyContext, needle: &str) -> bool {
        binds(ctx)
            .iter()
            .any(|b| b.desc.to_lowercase().contains(needle))
    }

    fn has_chord(ctx: KeyContext, chord: KeyChord) -> bool {
        binds(ctx).iter().any(|b| b.chords.contains(&chord))
    }

    #[test]
    fn keybinds_for_non_empty_for_every_context() {
        for &ctx in ALL_CONTEXTS {
            let groups = keybinds_for(ctx);
            assert!(!groups.is_empty(), "no groups for {ctx:?}");
            assert!(
                groups.iter().any(|g| !g.binds.is_empty()),
                "no binds for {ctx:?}"
            );
        }
    }

    #[test]
    fn context_label_non_empty_for_every_context() {
        for &ctx in ALL_CONTEXTS {
            assert!(!context_label(ctx).is_empty(), "empty label for {ctx:?}");
        }
    }

    #[test]
    fn types_has_wrap_and_help() {
        assert!(
            has_desc_containing(KeyContext::Types, "wrap"),
            "Types should expose the `x` wrap toggle"
        );
        assert!(
            has_chord(KeyContext::Types, k(KeyCode::Char('x'))),
            "Types should bind `x`"
        );
        assert!(
            has_chord(KeyContext::Types, k(KeyCode::Char('?'))),
            "Types should bind `?` help"
        );
    }

    #[test]
    fn graph_has_no_wrap_bind() {
        assert!(
            !has_chord(KeyContext::Graph, k(KeyCode::Char('x'))),
            "Graph does not handle `x`"
        );
        assert!(
            !has_desc_containing(KeyContext::Graph, "wrap"),
            "Graph has no wrap action"
        );
    }

    #[test]
    fn settings_nav_has_no_search_bind() {
        assert!(
            !has_chord(KeyContext::Settings, k(KeyCode::Char('/'))),
            "Settings nav does not handle `/` search"
        );
    }

    // ---- T5: handler <-> registry parity (both directions) -----------------
    //
    // For EVERY KeyContext, the set of keys the live handler ACTS ON must equal
    // the set of keys the registry DOCUMENTS:
    //   * registry subset of handled -> no dead help rows.
    //   * handled subset of registry -> no undocumented keys.
    //
    // "Acts on a key" = pressing it changes the observable App fingerprint (see
    // `App::fingerprint`). Each context is freshly seeded (T2 patterns, extended
    // so every registered key is LIVE) and every candidate chord is pressed
    // against a fresh app; the resulting Actual token set is compared to the
    // Expected set derived from the registry. All drift across all contexts is
    // collected and reported in one panic.

    use crossterm::event::KeyModifiers;

    /// A token in the comparison space: a specific chord, the printable-char
    /// catch-all that a `char_catchall` bind stands for, or the any-other-key
    /// catch-all that an `any_key` bind stands for.
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Token {
        Explicit { code: String, ctrl: bool },
        Catchall,
        AnyKey,
    }

    fn explicit_token(chord: KeyChord) -> Token {
        Token::Explicit {
            code: format!("{:?}", chord.code),
            ctrl: chord.ctrl,
        }
    }

    /// Every explicit chord across all binds of a context.
    fn explicit_chords(ctx: KeyContext) -> Vec<KeyChord> {
        let mut out = Vec::new();
        for b in binds(ctx) {
            out.extend_from_slice(b.chords);
        }
        out
    }

    fn has_catchall(ctx: KeyContext) -> bool {
        binds(ctx).iter().any(|b| b.char_catchall)
    }

    fn has_any_key(ctx: KeyContext) -> bool {
        binds(ctx).iter().any(|b| b.any_key)
    }

    /// The candidate chord set C: alphanumerics + every punctuation char that
    /// appears as a `Char(_)` chord anywhere in the registry, plus the non-char
    /// keys, each crossed with {NONE, CONTROL}. Derived, not hardcoded.
    fn candidate_chords() -> Vec<(KeyCode, bool)> {
        let mut chars: std::collections::BTreeSet<char> = std::collections::BTreeSet::new();
        for c in 'a'..='z' {
            chars.insert(c);
        }
        for c in 'A'..='Z' {
            chars.insert(c);
        }
        for c in '0'..='9' {
            chars.insert(c);
        }
        // Every distinct punctuation Char(_) the registry uses in ANY context
        // (so `/ ? backtick space` etc. are covered automatically).
        for &ctx in ALL_CONTEXTS {
            for b in binds(ctx) {
                for ch in b.chords {
                    if let KeyCode::Char(c) = ch.code {
                        chars.insert(c);
                    }
                }
            }
        }

        let mut codes: Vec<KeyCode> = chars.into_iter().map(KeyCode::Char).collect();
        codes.extend([
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Backspace,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Char(' '), // Space is Char(' ')
        ]);

        let mut out = Vec::new();
        for code in codes {
            out.push((code, false));
            out.push((code, true));
        }
        out
    }

    /// Map a pressed chord to its comparison token, in the token space users
    /// actually experience. The folds, in order:
    ///
    ///   1. An explicit registry chord maps to itself (Ctrl-d/u/k/j/s/c stay
    ///      distinct -- they ARE registered ctrl chords).
    ///   2. CONTROL fold: a `(code, ctrl:true)` press that is NOT a registered
    ///      ctrl chord, where `(code, ctrl:false)` IS handled by dispatch, folds
    ///      onto the plain `(code, false)` token. Rationale: most handlers
    ///      `match key.code` and ignore CONTROL, so crossterm's Ctrl-j (which
    ///      arrives as Char('j')+CONTROL) behaves identically to j -- one action,
    ///      one help row. We recurse so the plain press resolves through the same
    ///      ladder (a registered plain chord, else catchall/any-key).
    ///   3. `Catchall`: an unregistered printable char in a `char_catchall`
    ///      context (Search's Ctrl-<other-char> typing lines up here too).
    ///   4. `AnyKey`: any other press in an `any_key` context (scaffold dismiss).
    ///   5. Otherwise the chord maps to itself. A ctrl-only press that is NOT
    ///      registered and whose plain twin is NOT handled lands here, so it
    ///      surfaces as undocumented -- that is genuine drift (a ctrl-only key
    ///      with no help row) and the test should fail.
    ///
    /// TRADEOFF (documented residual false-pass): step 2 folds only when the plain
    /// twin is handled, so the "registry lists plain X but the handler matches only
    /// Ctrl-X" shape does NOT fold -- the parity test catches it directly (Ctrl-X
    /// surfaces as undocumented, plain X as a dead row). The genuine residual is
    /// narrower: an UNregistered Ctrl-X arm doing something distinct while its plain
    /// twin X is also handled folds onto X and is masked. Mitigation: every real
    /// ctrl binding is a registered explicit `ctrl(...)` chord, so it is compared
    /// distinctly (step 1) and never reaches the fold.
    fn token_for(
        ctx: KeyContext,
        code: KeyCode,
        ctrl: bool,
        explicit: &[KeyChord],
        plain_handled: &dyn Fn(KeyCode) -> bool,
    ) -> Token {
        let chord = KeyChord { code, ctrl };
        if explicit.contains(&chord) {
            return Token::Explicit {
                code: format!("{:?}", code),
                ctrl,
            };
        }
        if ctrl && plain_handled(code) {
            return token_for(ctx, code, false, explicit, plain_handled);
        }
        if has_catchall(ctx) && matches!(code, KeyCode::Char(_)) {
            return Token::Catchall;
        }
        if has_any_key(ctx) {
            return Token::AnyKey;
        }
        Token::Explicit {
            code: format!("{:?}", code),
            ctrl,
        }
    }

    /// Did pressing `(code, ctrl)` change the seeded app's fingerprint?
    fn handled(ctx: KeyContext, code: KeyCode, ctrl: bool) -> bool {
        use crate::tui::state::parity_seed;
        let (tmp, mut app, config) = parity_seed::seed(ctx);
        let root = tmp.path().to_path_buf();
        let before = app.key_fingerprint();
        let mods = if ctrl {
            KeyModifiers::CONTROL
        } else {
            KeyModifiers::NONE
        };
        app.handle_key(code, mods, &root, &config);
        let after = app.key_fingerprint();
        drop(tmp);
        before != after
    }

    #[test]
    fn registry_matches_handlers_for_every_context() {
        let candidates = candidate_chords();
        let mut report = String::new();

        for &ctx in ALL_CONTEXTS {
            let explicit = explicit_chords(ctx);

            // Expected = explicit binds + catchall/any-key tokens for those flags.
            let mut expected: std::collections::BTreeSet<Token> =
                explicit.iter().copied().map(explicit_token).collect();
            if has_catchall(ctx) {
                expected.insert(Token::Catchall);
            }
            if has_any_key(ctx) {
                expected.insert(Token::AnyKey);
            }

            // The CONTROL fold consults whether a key's PLAIN twin is handled, so
            // precompute the plain-handled set once per context.
            let plain_handled_codes: std::collections::BTreeSet<String> = candidates
                .iter()
                .filter(|&&(_, ctrl)| !ctrl)
                .filter(|&&(code, _)| handled(ctx, code, false))
                .map(|&(code, _)| format!("{:?}", code))
                .collect();
            let plain_handled =
                |code: KeyCode| plain_handled_codes.contains(&format!("{:?}", code));

            // Actual = tokens of every candidate the handler acts on.
            let mut actual: std::collections::BTreeSet<Token> = std::collections::BTreeSet::new();
            for &(code, ctrl) in &candidates {
                if handled(ctx, code, ctrl) {
                    actual.insert(token_for(ctx, code, ctrl, &explicit, &plain_handled));
                }
            }

            let dead_rows: Vec<&Token> = expected.difference(&actual).collect();
            let undocumented: Vec<&Token> = actual.difference(&expected).collect();

            if !dead_rows.is_empty() || !undocumented.is_empty() {
                report.push_str(&format!("\n[{}] ({:?})\n", context_label(ctx), ctx));
                if !dead_rows.is_empty() {
                    report.push_str(&format!(
                        "  registry-only (dead help rows -- documented but handler ignores): {:?}\n",
                        dead_rows
                    ));
                }
                if !undocumented.is_empty() {
                    report.push_str(&format!(
                        "  handler-only (undocumented -- key acts but no help row): {:?}\n",
                        undocumented
                    ));
                }
            }
        }

        assert!(
            report.is_empty(),
            "keybind registry <-> handler parity drift:\n{report}"
        );
    }

    /// The display string must not advertise a single-character key the handler
    /// never matches. For every context, every single-ASCII-alphanumeric token
    /// in a bind's display `keys` must correspond to a `Char(c)` chord (ctrl
    /// either way) or be covered by `char_catchall`. Multi-char tokens (`Up`,
    /// `Esc`, `Ctrl-d`, ...) are skipped -- we don't parse modifiers.
    #[test]
    fn display_matches_chords() {
        for &ctx in ALL_CONTEXTS {
            for bind in binds(ctx) {
                for token in bind.keys.split(['/', ' ', '\t']) {
                    let mut chars = token.chars();
                    let (Some(c), None) = (chars.next(), chars.next()) else {
                        continue;
                    };
                    if !c.is_ascii_alphanumeric() {
                        continue;
                    }
                    let has_chord = bind.chords.iter().any(|ch| ch.code == KeyCode::Char(c));
                    assert!(
                        has_chord || bind.char_catchall,
                        "{ctx:?}: display \"{}\" names '{c}' but no Char('{c}') chord",
                        bind.keys
                    );
                }
            }
        }
    }
}
