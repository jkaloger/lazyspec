use std::io::{IsTerminal, Write};
use std::thread::sleep;
use std::time::Duration;

use console::Style;
use crossterm::{cursor, execute, terminal};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::engine::reservation::ReservationProgress;
use crate::spinners::{spinner, FrameColour, SpinnerState};

const TICK: Duration = Duration::from_millis(120);

/// Delay between spoken words in the greeting animation. Within the RFC's
/// 75-200ms window: slow enough to read the mouth cycle, brisk enough to finish.
const WORD_TICK: Duration = Duration::from_millis(110);

/// Map a semantic [`FrameColour`] to a `console` style, mirroring the TUI's
/// `frame_style`: Accent is the terminal default so both light and dark themes
/// stay legible, with dim/bold and green/red carrying the remaining cues.
pub fn frame_style(colour: FrameColour) -> Style {
    match colour {
        FrameColour::Accent => Style::new(),
        FrameColour::Dim => Style::new().dim(),
        FrameColour::Success => Style::new().green(),
        FrameColour::Error => Style::new().red(),
    }
}

/// A stderr spinner animating the compact loading face, or `None` when output
/// must stay machine-readable (`--json`) or stderr is not a terminal. `None` is
/// the signal for callers to stay silent, so stdout parity is preserved.
pub fn op_spinner(msg: impl Into<String>, json: bool) -> Option<ProgressBar> {
    if json || !std::io::stderr().is_terminal() {
        return None;
    }

    let face = spinner("face");
    // indicatif treats the trailing tick as the finished frame; the loop cycles
    // the four loading frames before it.
    let mut ticks: Vec<String> = (0..4)
        .map(|i| {
            let frame = face.compact(SpinnerState::Loading, i);
            frame_style(frame.colour)
                .apply_to(&frame.lines[0])
                .to_string()
        })
        .collect();
    ticks.push(face.compact(SpinnerState::Success, 0).lines[0].clone());
    let tick_refs: Vec<&str> = ticks.iter().map(String::as_str).collect();

    let style = ProgressStyle::with_template("{spinner} {msg}")
        .expect("static spinner template is valid")
        .tick_strings(&tick_refs);

    let pb = ProgressBar::new_spinner().with_message(msg.into());
    pb.set_style(style);
    pb.set_draw_target(ProgressDrawTarget::stderr());
    pb.enable_steady_tick(TICK);
    Some(pb)
}

/// A human-readable message for each reservation progress point, fed to the
/// spinner as the underlying op advances.
pub fn reservation_message(progress: &ReservationProgress) -> String {
    match progress {
        ReservationProgress::QueryingRemote => "querying remote for reserved ids".to_string(),
        ReservationProgress::PushAttempt {
            attempt,
            max,
            candidate,
        } => format!("reserving {candidate} (attempt {attempt}/{max})"),
        ReservationProgress::PushRejected { candidate } => format!("{candidate} taken, retrying"),
        ReservationProgress::Reserved { number } => format!("reserved {number}"),
    }
}

/// Clear the spinner and print a green check completion line to stderr. A no-op
/// when the spinner was suppressed, keeping non-interactive output untouched.
pub fn finish_ok(pb: Option<ProgressBar>, msg: &str) {
    if let Some(pb) = pb {
        pb.finish_and_clear();
        eprintln!("{}", crate::cli::style::success_line(msg));
    }
}

/// Clear the spinner and print a red cross failure line to stderr. A no-op when
/// the spinner was suppressed.
pub fn finish_err(pb: Option<ProgressBar>, msg: &str) {
    if let Some(pb) = pb {
        pb.finish_and_clear();
        eprintln!("{} {}", crate::cli::style::error_prefix(), msg);
    }
}

/// Whether the interactive `init` wizard should play its animated greeting: only
/// on a colour-capable TTY that is not emitting machine-readable output. Pure so
/// the guard is testable without a terminal; the `say` animation itself assumes
/// the caller has already cleared this gate (dictum 2).
pub fn should_greet(json: bool, is_tty: bool, colors: bool) -> bool {
    !json && is_tty && colors
}

/// Hand-rolled talking-face greeting for the interactive init wizard -- the
/// Houston `say` equivalent. Draws the full-box face and speaks `msg` word by
/// word, cycling the loading eyes/mouth on each word, then settles on the happy
/// success face. This is a bespoke per-word animation, distinct from the
/// steady-tick op spinner. Callers must gate on [`should_greet`]; this always
/// draws (and emits ESC bytes), so it must never run under `--json`/non-TTY.
pub fn say(msg: &str) {
    let face = spinner("face");
    let words: Vec<&str> = msg.split_whitespace().collect();
    let mut out = std::io::stdout();

    let draw = |out: &mut std::io::Stdout, state: SpinnerState, idx: u64, spoken: &str| {
        let frame = face.full(state, idx);
        let style = frame_style(frame.colour);
        let _ = writeln!(out, "{}", style.apply_to(&frame.lines[0]));
        let _ = writeln!(out, "{}  {}", style.apply_to(&frame.lines[1]), spoken);
        let _ = writeln!(out, "{}", style.apply_to(&frame.lines[2]));
        let _ = out.flush();
    };

    let redraw = |out: &mut std::io::Stdout, state: SpinnerState, idx: u64, spoken: &str| {
        let _ = execute!(
            out,
            cursor::MoveToPreviousLine(3),
            terminal::Clear(terminal::ClearType::FromCursorDown)
        );
        draw(out, state, idx, spoken);
    };

    let mut spoken = String::new();
    draw(&mut out, SpinnerState::Loading, 0, &spoken);
    for (i, word) in words.iter().enumerate() {
        sleep(WORD_TICK);
        if !spoken.is_empty() {
            spoken.push(' ');
        }
        spoken.push_str(word);
        redraw(&mut out, SpinnerState::Loading, i as u64 + 1, &spoken);
    }

    sleep(WORD_TICK);
    redraw(&mut out, SpinnerState::Success, 0, &spoken);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_spinner_none_under_json() {
        assert!(op_spinner("working", true).is_none());
    }

    #[test]
    fn op_spinner_none_when_stderr_not_a_terminal() {
        // The test harness captures stderr, so it is never a TTY here; the guard
        // must suppress the spinner even when json is false.
        assert!(op_spinner("working", false).is_none());
    }

    // Dictum 2: the greeting plays only on a colour TTY without `--json`; every
    // suppressing condition (json, non-TTY, colours off) gates it out so no ESC
    // bytes reach machine-readable or piped output.
    #[test]
    fn should_greet_only_on_colour_tty_without_json() {
        assert!(should_greet(false, true, true), "colour tty, no json");
        assert!(!should_greet(true, true, true), "json suppresses");
        assert!(!should_greet(false, false, true), "non-tty suppresses");
        assert!(!should_greet(false, true, false), "colours off suppresses");
    }

    #[test]
    fn frame_style_maps_each_colour() {
        assert_eq!(frame_style(FrameColour::Accent), Style::new());
        assert_eq!(frame_style(FrameColour::Dim), Style::new().dim());
        assert_eq!(frame_style(FrameColour::Success), Style::new().green());
        assert_eq!(frame_style(FrameColour::Error), Style::new().red());
    }

    #[test]
    fn reservation_progress_maps_to_spinner_state() {
        assert_eq!(
            ReservationProgress::QueryingRemote.spinner_state(),
            SpinnerState::Loading
        );
        assert_eq!(
            ReservationProgress::PushAttempt {
                attempt: 1,
                max: 3,
                candidate: 7
            }
            .spinner_state(),
            SpinnerState::Loading
        );
        assert_eq!(
            ReservationProgress::PushRejected { candidate: 7 }.spinner_state(),
            SpinnerState::Loading
        );
        assert_eq!(
            ReservationProgress::Reserved { number: 7 }.spinner_state(),
            SpinnerState::Success
        );
    }

    #[test]
    fn reservation_message_describes_each_point() {
        assert_eq!(
            reservation_message(&ReservationProgress::QueryingRemote),
            "querying remote for reserved ids"
        );
        assert_eq!(
            reservation_message(&ReservationProgress::PushAttempt {
                attempt: 2,
                max: 5,
                candidate: 9
            }),
            "reserving 9 (attempt 2/5)"
        );
        assert_eq!(
            reservation_message(&ReservationProgress::PushRejected { candidate: 9 }),
            "9 taken, retrying"
        );
        assert_eq!(
            reservation_message(&ReservationProgress::Reserved { number: 9 }),
            "reserved 9"
        );
    }
}
