use crate::cli::style::{bold, dim};
use anyhow::{bail, Result};
use std::io::{BufRead, Write};

/// The test seam for interactive prompts: the real terminal implementation and
/// the scripted test fake both satisfy it, so `run_add_type_interactive` never
/// touches stdin/stdout directly.
pub trait Prompter {
    /// Ask for a free-text value. A blank answer yields `default` when one is
    /// given, otherwise the raw (empty) input.
    fn ask(&mut self, label: &str, default: Option<&str>) -> Result<String>;
    /// Ask a yes/no question. Blank yields `default`.
    fn confirm(&mut self, label: &str, default: bool) -> Result<bool>;
    /// Ask to choose one of `options` as a numbered chooser. A blank answer
    /// yields `default`; otherwise a 1-based number resolves into `options` or an
    /// exact option string is matched. The real (`StdinPrompter`) impl rejects an
    /// out-of-list answer and re-asks; the scripted fake returns the queued answer
    /// verbatim (validation lives at the callsite, not the fake).
    fn select(&mut self, label: &str, options: &[&str], default: &str) -> Result<String>;
    /// Ask to choose several values in one prompt. When `options` is empty the
    /// answer is freeform: the single input line is split on `,`, each token
    /// trimmed and empties dropped (used for user-invented lifecycle states). When
    /// `options` is non-empty it is a numbered multi-chooser accepting a
    /// comma-separated mix of 1-based numbers and exact option strings; the real
    /// impl rejects any unknown token and re-asks. A blank answer yields
    /// `defaults`. Selections are returned in input order, de-duplicated.
    fn multi_select(
        &mut self,
        label: &str,
        options: &[&str],
        defaults: &[&str],
    ) -> Result<Vec<String>>;
}

/// Real prompter: renders the label (and any default) to a writer and reads one
/// line from a reader. Generic over the streams so it stays testable, though in
/// practice it wraps stdin/stdout.
pub struct StdinPrompter<R: BufRead, W: Write> {
    reader: R,
    writer: W,
}

impl StdinPrompter<std::io::BufReader<std::io::Stdin>, std::io::Stdout> {
    pub fn new() -> Self {
        StdinPrompter {
            reader: std::io::BufReader::new(std::io::stdin()),
            writer: std::io::stdout(),
        }
    }
}

impl Default for StdinPrompter<std::io::BufReader<std::io::Stdin>, std::io::Stdout> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: BufRead, W: Write> StdinPrompter<R, W> {
    fn read_line(&mut self, prompt: &str) -> Result<String> {
        write!(self.writer, "{prompt}")?;
        self.writer.flush()?;
        let mut line = String::new();
        let n = self.reader.read_line(&mut line)?;
        if n == 0 {
            bail!("unexpected end of input while prompting");
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }
}

impl<R: BufRead, W: Write> Prompter for StdinPrompter<R, W> {
    fn ask(&mut self, label: &str, default: Option<&str>) -> Result<String> {
        let prompt = match default {
            Some(d) if !d.is_empty() => {
                format!("{} {}: ", bold(label), dim(&format!("[{d}]")))
            }
            _ => format!("{}: ", bold(label)),
        };
        let input = self.read_line(&prompt)?;
        if input.is_empty() {
            if let Some(d) = default {
                return Ok(d.to_string());
            }
        }
        Ok(input)
    }

    fn confirm(&mut self, label: &str, default: bool) -> Result<bool> {
        let hint = if default { "Y/n" } else { "y/N" };
        let input = self.read_line(&format!("{} {}: ", bold(label), dim(&format!("[{hint}]"))))?;
        Ok(match input.to_ascii_lowercase().as_str() {
            "" => default,
            "y" | "yes" => true,
            _ => false,
        })
    }

    fn select(&mut self, label: &str, options: &[&str], default: &str) -> Result<String> {
        loop {
            writeln!(self.writer, "{}:", bold(label))?;
            for (i, opt) in options.iter().enumerate() {
                let cue = if *opt == default {
                    format!(" {}", dim("[default]"))
                } else {
                    String::new()
                };
                writeln!(self.writer, "  {}) {opt}{cue}", i + 1)?;
            }
            let input = self.read_line(&format!("Choose {}: ", dim(&format!("[{default}]"))))?;
            if input.is_empty() {
                return Ok(default.to_string());
            }
            if let Some(opt) = resolve_choice(&input, options) {
                return Ok(opt);
            }
            writeln!(
                self.writer,
                "\"{input}\" is not one of the options; choose a number or an exact name"
            )?;
        }
    }

    fn multi_select(
        &mut self,
        label: &str,
        options: &[&str],
        defaults: &[&str],
    ) -> Result<Vec<String>> {
        if options.is_empty() {
            let input = self.read_line(&format!("{label} (comma-separated): "))?;
            if input.trim().is_empty() {
                return Ok(defaults.iter().map(|s| s.to_string()).collect());
            }
            return Ok(dedup(split_csv(&input)));
        }
        loop {
            writeln!(self.writer, "{label} (comma-separated):")?;
            for (i, opt) in options.iter().enumerate() {
                writeln!(self.writer, "  {}) {opt}", i + 1)?;
            }
            let hint = defaults.join(", ");
            let input = self.read_line(&format!("Choose [{hint}]: "))?;
            if input.trim().is_empty() {
                return Ok(defaults.iter().map(|s| s.to_string()).collect());
            }
            let mut chosen: Vec<String> = Vec::new();
            let mut bad: Option<String> = None;
            for token in input.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                match resolve_choice(token, options) {
                    Some(opt) => chosen.push(opt),
                    None => {
                        bad = Some(token.to_string());
                        break;
                    }
                }
            }
            if let Some(token) = bad {
                writeln!(
                    self.writer,
                    "\"{token}\" is not one of the options; choose numbers or exact names"
                )?;
                continue;
            }
            return Ok(dedup(chosen));
        }
    }
}

/// Resolve a single token to an option: a 1-based index into `options`, or an
/// exact option string. Returns the canonical option text.
fn resolve_choice(token: &str, options: &[&str]) -> Option<String> {
    if let Ok(n) = token.parse::<usize>() {
        if n >= 1 && n <= options.len() {
            return Some(options[n - 1].to_string());
        }
    }
    options.iter().find(|o| **o == token).map(|o| o.to_string())
}

fn split_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for item in items {
        if !seen.contains(&item) {
            seen.push(item);
        }
    }
    seen
}

/// Test fake: answers are dequeued in order from a queue. `confirm` reads a
/// `y`/`yes` (case-insensitive) as true, anything else as false; `select`
/// returns the queued answer verbatim (blank falls back to the default).
/// `multi_select` pops one queued answer: blank falls back to `defaults`,
/// otherwise the line is split on `,` (each token trimmed, empties dropped) into
/// the selection list. No validation happens in the fake -- it stays verbatim so
/// callsite re-ask loops remain the only test seam.
pub struct ScriptedPrompter {
    answers: std::collections::VecDeque<String>,
}

impl ScriptedPrompter {
    pub fn new(answers: Vec<String>) -> Self {
        ScriptedPrompter {
            answers: answers.into(),
        }
    }

    fn pop(&mut self, label: &str) -> Result<String> {
        match self.answers.pop_front() {
            Some(a) => Ok(a),
            None => bail!("scripted prompter ran out of answers at \"{label}\""),
        }
    }
}

impl Prompter for ScriptedPrompter {
    fn ask(&mut self, label: &str, default: Option<&str>) -> Result<String> {
        let input = self.pop(label)?;
        if input.is_empty() {
            if let Some(d) = default {
                return Ok(d.to_string());
            }
        }
        Ok(input)
    }

    fn confirm(&mut self, label: &str, default: bool) -> Result<bool> {
        let input = self.pop(label)?;
        Ok(match input.to_ascii_lowercase().as_str() {
            "" => default,
            "y" | "yes" => true,
            _ => false,
        })
    }

    fn select(&mut self, label: &str, _options: &[&str], default: &str) -> Result<String> {
        let input = self.pop(label)?;
        Ok(if input.is_empty() {
            default.to_string()
        } else {
            input
        })
    }

    fn multi_select(
        &mut self,
        label: &str,
        _options: &[&str],
        defaults: &[&str],
    ) -> Result<Vec<String>> {
        let input = self.pop(label)?;
        if input.is_empty() {
            return Ok(defaults.iter().map(|s| s.to_string()).collect());
        }
        Ok(split_csv(&input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn stdin(input: &str) -> StdinPrompter<Cursor<Vec<u8>>, Vec<u8>> {
        StdinPrompter {
            reader: Cursor::new(input.as_bytes().to_vec()),
            writer: Vec::new(),
        }
    }

    fn written(p: &StdinPrompter<Cursor<Vec<u8>>, Vec<u8>>) -> String {
        String::from_utf8(p.writer.clone()).unwrap()
    }

    // Real select rejects an out-of-list answer and re-asks, accepting the valid
    // one on the next line; a rejection cue is emitted.
    #[test]
    fn real_select_rejects_out_of_list_then_accepts() {
        let mut p = stdin("bogus\nfilesystem\n");
        let choice = p
            .select("Store", &["filesystem", "git-ref"], "filesystem")
            .unwrap();
        assert_eq!(choice, "filesystem");
        assert!(
            written(&p).contains("is not one of the options"),
            "expected a rejection cue, got: {}",
            written(&p)
        );
    }

    // Real select resolves a 1-based number to the matching option.
    #[test]
    fn real_select_resolves_number() {
        let mut p = stdin("2\n");
        let choice = p
            .select("Store", &["filesystem", "git-ref"], "filesystem")
            .unwrap();
        assert_eq!(choice, "git-ref");
    }

    // Real select: a blank line yields the default.
    #[test]
    fn real_select_blank_yields_default() {
        let mut p = stdin("\n");
        let choice = p
            .select("Store", &["filesystem", "git-ref"], "filesystem")
            .unwrap();
        assert_eq!(choice, "filesystem");
    }

    // Real multi_select over a fixed option set captures several by number and
    // returns them in input order.
    #[test]
    fn real_multi_select_numbers_capture_several() {
        let mut p = stdin("1,3\n");
        let chosen = p.multi_select("Pick", &["a", "b", "c"], &[]).unwrap();
        assert_eq!(chosen, vec!["a".to_string(), "c".to_string()]);
    }

    // Real multi_select rejects an unknown token (number out of range or unknown
    // name) and re-asks.
    #[test]
    fn real_multi_select_rejects_unknown_token() {
        let mut p = stdin("9\nnope\n1\n");
        let chosen = p.multi_select("Pick", &["a", "b", "c"], &[]).unwrap();
        assert_eq!(chosen, vec!["a".to_string()]);
        assert!(written(&p).contains("is not one of the options"));
    }

    // Real multi_select with empty options is freeform: split on commas, trim.
    #[test]
    fn real_multi_select_freeform_splits_and_trims() {
        let mut p = stdin("draft, accepted\n");
        let chosen = p.multi_select("Lifecycle states", &[], &[]).unwrap();
        assert_eq!(chosen, vec!["draft".to_string(), "accepted".to_string()]);
    }

    // Scripted multi_select splits a queued answer; a blank falls back to defaults.
    #[test]
    fn scripted_multi_select_splits_and_defaults() {
        let mut p = ScriptedPrompter::new(vec!["a,b,c".to_string()]);
        assert_eq!(
            p.multi_select("Pick", &[], &[]).unwrap(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );

        let mut p = ScriptedPrompter::new(vec![String::new()]);
        assert_eq!(
            p.multi_select("Pick", &[], &["x", "y"]).unwrap(),
            vec!["x".to_string(), "y".to_string()]
        );
    }
}
