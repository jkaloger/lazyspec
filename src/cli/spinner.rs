use std::io::IsTerminal;
use std::time::Duration;

use console::Style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::engine::reservation::ReservationProgress;
use crate::spinners::{spinner, FrameColour, SpinnerState};

const TICK: Duration = Duration::from_millis(120);

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
