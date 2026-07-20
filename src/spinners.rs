//! Pure spinner framework shared by the TUI and CLI.
//!
//! This module carries no terminal dependency: frames are plain strings tagged
//! with a semantic [`FrameColour`]. Consumers map colour to their own styling.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinnerState {
    Idle,
    Loading,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameColour {
    Accent,
    Dim,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub lines: Vec<String>,
    pub colour: FrameColour,
}

pub trait Spinner {
    fn compact(&self, state: SpinnerState, idx: u64) -> Frame;
    fn full(&self, state: SpinnerState, idx: u64) -> Frame;
}

pub struct FaceSpinner {
    pub ascii: bool,
}

impl FaceSpinner {
    fn borders(&self) -> (&'static str, &'static str) {
        if self.ascii {
            ("+-------+", "+-------+")
        } else {
            ("╭───────╮", "╰───────╯")
        }
    }

    fn full_frames(&self, state: SpinnerState) -> (&'static [&'static str], FrameColour) {
        match (self.ascii, state) {
            (false, SpinnerState::Idle) => (&["│ ● ▪ ● │", "│ - ▪ - │"], FrameColour::Accent),
            (false, SpinnerState::Loading) => (
                &["│ ◐ ○ ◐ │", "│ ◓ ○ ◓ │", "│ ◑ ○ ◑ │", "│ ◒ ○ ◒ │"],
                FrameColour::Accent,
            ),
            (false, SpinnerState::Success) => (&["│ ◠ ◡ ◠ │"], FrameColour::Success),
            (false, SpinnerState::Error) => (&["│ × ▂ × │"], FrameColour::Error),
            (true, SpinnerState::Idle) => (&["| o - o |", "| - - - |"], FrameColour::Accent),
            (true, SpinnerState::Loading) => (
                &["| | o | |", "| / o / |", "| - o - |", "| \\ o \\ |"],
                FrameColour::Accent,
            ),
            (true, SpinnerState::Success) => (&["| ^ _ ^ |"], FrameColour::Success),
            (true, SpinnerState::Error) => (&["| x _ x |"], FrameColour::Error),
        }
    }

    fn compact_frames(&self, state: SpinnerState) -> (&'static [&'static str], FrameColour) {
        match (self.ascii, state) {
            (false, SpinnerState::Idle) => (&["[·‿·]"], FrameColour::Accent),
            (false, SpinnerState::Loading) => {
                (&["[◐‿◐]", "[◓‿◓]", "[◑‿◑]", "[◒‿◒]"], FrameColour::Accent)
            }
            (false, SpinnerState::Success) => (&["[◠‿◠]"], FrameColour::Success),
            (false, SpinnerState::Error) => (&["[×_×]"], FrameColour::Error),
            (true, SpinnerState::Idle) => (&["[o_o]"], FrameColour::Accent),
            (true, SpinnerState::Loading) => {
                (&["[|_|]", "[/_/]", "[-_-]", "[\\_\\]"], FrameColour::Accent)
            }
            (true, SpinnerState::Success) => (&["[^_^]"], FrameColour::Success),
            (true, SpinnerState::Error) => (&["[x_x]"], FrameColour::Error),
        }
    }
}

fn pick<'a>(frames: &'a [&'a str], idx: u64) -> &'a str {
    frames[(idx % frames.len() as u64) as usize]
}

impl Spinner for FaceSpinner {
    fn compact(&self, state: SpinnerState, idx: u64) -> Frame {
        let (frames, colour) = self.compact_frames(state);
        Frame {
            lines: vec![pick(frames, idx).to_string()],
            colour,
        }
    }

    fn full(&self, state: SpinnerState, idx: u64) -> Frame {
        let (frames, colour) = self.full_frames(state);
        let (top, bottom) = self.borders();
        Frame {
            lines: vec![
                top.to_string(),
                pick(frames, idx).to_string(),
                bottom.to_string(),
            ],
            colour,
        }
    }
}

static FACE: FaceSpinner = FaceSpinner { ascii: false };

pub fn spinner(_name: &str) -> &'static dyn Spinner {
    &FACE
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_STATES: [SpinnerState; 4] = [
        SpinnerState::Idle,
        SpinnerState::Loading,
        SpinnerState::Success,
        SpinnerState::Error,
    ];

    #[test]
    fn full_frame_catalogue_and_wrap() {
        let s = FaceSpinner { ascii: false };

        assert_eq!(s.full(SpinnerState::Idle, 0).lines[1], "│ ● ▪ ● │");
        assert_eq!(s.full(SpinnerState::Idle, 1).lines[1], "│ - ▪ - │");
        assert_eq!(s.full(SpinnerState::Idle, 2), s.full(SpinnerState::Idle, 0));

        assert_eq!(s.full(SpinnerState::Loading, 0).lines[1], "│ ◐ ○ ◐ │");
        assert_eq!(s.full(SpinnerState::Loading, 1).lines[1], "│ ◓ ○ ◓ │");
        assert_eq!(s.full(SpinnerState::Loading, 2).lines[1], "│ ◑ ○ ◑ │");
        assert_eq!(s.full(SpinnerState::Loading, 3).lines[1], "│ ◒ ○ ◒ │");
        assert_eq!(
            s.full(SpinnerState::Loading, 4),
            s.full(SpinnerState::Loading, 0)
        );

        assert_eq!(s.full(SpinnerState::Success, 0).lines[1], "│ ◠ ◡ ◠ │");
        assert_eq!(
            s.full(SpinnerState::Success, 999),
            s.full(SpinnerState::Success, 0)
        );

        assert_eq!(s.full(SpinnerState::Error, 0).lines[1], "│ × ▂ × │");
        assert_eq!(
            s.full(SpinnerState::Error, 999),
            s.full(SpinnerState::Error, 0)
        );

        for st in ALL_STATES {
            let f = s.full(st, 0);
            assert_eq!(f.lines[0], "╭───────╮");
            assert_eq!(f.lines[2], "╰───────╯");
        }
    }

    #[test]
    fn compact_frame_catalogue_and_wrap() {
        let s = FaceSpinner { ascii: false };

        assert_eq!(s.compact(SpinnerState::Idle, 0).lines[0], "[·‿·]");
        assert_eq!(
            s.compact(SpinnerState::Idle, 1),
            s.compact(SpinnerState::Idle, 0)
        );

        assert_eq!(s.compact(SpinnerState::Loading, 0).lines[0], "[◐‿◐]");
        assert_eq!(s.compact(SpinnerState::Loading, 1).lines[0], "[◓‿◓]");
        assert_eq!(s.compact(SpinnerState::Loading, 2).lines[0], "[◑‿◑]");
        assert_eq!(s.compact(SpinnerState::Loading, 3).lines[0], "[◒‿◒]");
        assert_eq!(
            s.compact(SpinnerState::Loading, 4),
            s.compact(SpinnerState::Loading, 0)
        );

        assert_eq!(s.compact(SpinnerState::Success, 0).lines[0], "[◠‿◠]");
        assert_eq!(
            s.compact(SpinnerState::Success, 999),
            s.compact(SpinnerState::Success, 0)
        );

        assert_eq!(s.compact(SpinnerState::Error, 0).lines[0], "[×_×]");
        assert_eq!(
            s.compact(SpinnerState::Error, 999),
            s.compact(SpinnerState::Error, 0)
        );
    }

    #[test]
    fn ascii_output_has_no_non_ascii_bytes() {
        let s = FaceSpinner { ascii: true };
        for st in ALL_STATES {
            for idx in 0..8u64 {
                let full = s.full(st, idx);
                assert!(
                    full.lines.iter().all(|l| l.is_ascii()),
                    "full {st:?} idx {idx} not ascii: {:?}",
                    full.lines
                );
                let compact = s.compact(st, idx);
                assert!(
                    compact.lines.iter().all(|l| l.is_ascii()),
                    "compact {st:?} idx {idx} not ascii: {:?}",
                    compact.lines
                );
            }
        }
    }

    #[test]
    fn full_box_width_invariant() {
        for ascii in [false, true] {
            let s = FaceSpinner { ascii };
            let mut widths = Vec::new();
            for st in ALL_STATES {
                for idx in 0..8u64 {
                    let f = s.full(st, idx);
                    assert_eq!(f.lines.len(), 3, "{st:?} idx {idx} not 3 lines");
                    for line in &f.lines {
                        widths.push(line.chars().count());
                    }
                }
            }
            assert!(
                widths.iter().all(|&w| w == widths[0]),
                "unequal line widths (ascii={ascii}): {widths:?}"
            );
        }
    }

    #[test]
    fn registry_defaults_to_unicode_face() {
        let unicode = FaceSpinner { ascii: false };
        let expected = unicode.full(SpinnerState::Loading, 0);
        assert_eq!(spinner("face").full(SpinnerState::Loading, 0), expected);
        assert_eq!(spinner("unknown").full(SpinnerState::Loading, 0), expected);
    }

    #[test]
    fn frame_colour_mapping_per_state() {
        for ascii in [false, true] {
            let s = FaceSpinner { ascii };
            for st in ALL_STATES {
                let expected = match st {
                    SpinnerState::Idle | SpinnerState::Loading => FrameColour::Accent,
                    SpinnerState::Success => FrameColour::Success,
                    SpinnerState::Error => FrameColour::Error,
                };
                assert_eq!(s.full(st, 0).colour, expected);
                assert_eq!(s.compact(st, 0).colour, expected);
            }
        }
    }
}
