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
    /// Ask to choose one of `options`. Blank yields `default`.
    fn select(&mut self, label: &str, options: &[&str], default: &str) -> Result<String>;
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
            Some(d) if !d.is_empty() => format!("{label} [{d}]: "),
            _ => format!("{label}: "),
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
        let input = self.read_line(&format!("{label} [{hint}]: "))?;
        Ok(match input.to_ascii_lowercase().as_str() {
            "" => default,
            "y" | "yes" => true,
            _ => false,
        })
    }

    fn select(&mut self, label: &str, options: &[&str], default: &str) -> Result<String> {
        let joined = options.join(", ");
        let input = self.read_line(&format!("{label} ({joined}) [{default}]: "))?;
        Ok(if input.is_empty() {
            default.to_string()
        } else {
            input
        })
    }
}

/// Test fake: answers are dequeued in order from a queue. `confirm` reads a
/// `y`/`yes` (case-insensitive) as true, anything else as false; `select`
/// returns the queued answer verbatim (blank falls back to the default).
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
}
