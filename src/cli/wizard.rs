use anyhow::{bail, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, MultiSelect, Select};

/// The test seam for interactive prompts: the real terminal implementation and
/// the scripted test fake both satisfy it, so `run_add_type_interactive` never
/// touches stdin/stdout directly.
pub trait Prompter {
    /// Ask for a free-text value. A blank answer yields `default` when one is
    /// given, otherwise the raw (empty) input.
    fn ask(&mut self, label: &str, default: Option<&str>) -> Result<String>;
    /// Ask a yes/no question. Blank yields `default`.
    fn confirm(&mut self, label: &str, default: bool) -> Result<bool>;
    /// Ask to choose one of `options`. The real (`StdinPrompter`) impl renders an
    /// arrow-key chooser (Enter picks) with the item equal to `default`
    /// pre-selected; the scripted fake returns the queued answer verbatim (a blank
    /// falls back to `default`; validation lives at the callsite, not the fake).
    fn select(&mut self, label: &str, options: &[&str], default: &str) -> Result<String>;
    /// Ask to choose several values in one prompt. When `options` is empty the
    /// answer is freeform: the single input line is split on `,`, each token
    /// trimmed and empties dropped (used for user-invented lifecycle states). When
    /// `options` is non-empty the real impl renders an arrow-key multi-chooser
    /// (Space toggles, Enter confirms) with the items in `defaults` pre-checked;
    /// the scripted fake splits its queued line on `,`. A blank/empty selection
    /// yields `defaults`. Selections are returned in option order, de-duplicated.
    fn multi_select(
        &mut self,
        label: &str,
        options: &[&str],
        defaults: &[&str],
    ) -> Result<Vec<String>>;
}

/// Real prompter: drives interactive `dialoguer` widgets styled with
/// `ColorfulTheme`, which shares `console`'s colour state so `NO_COLOR`/`CLICOLOR`
/// are honoured. `dialoguer` owns the terminal, navigation, and validation.
pub struct StdinPrompter;

impl StdinPrompter {
    pub fn new() -> Self {
        StdinPrompter
    }
}

impl Default for StdinPrompter {
    fn default() -> Self {
        Self::new()
    }
}

impl Prompter for StdinPrompter {
    fn ask(&mut self, label: &str, default: Option<&str>) -> Result<String> {
        let theme = ColorfulTheme::default();
        let value: String = match default {
            Some(d) if !d.is_empty() => Input::with_theme(&theme)
                .with_prompt(label)
                .allow_empty(true)
                .default(d.to_string())
                .interact_text()?,
            _ => Input::with_theme(&theme)
                .with_prompt(label)
                .allow_empty(true)
                .interact_text()?,
        };
        if value.is_empty() {
            if let Some(d) = default {
                return Ok(d.to_string());
            }
        }
        Ok(value)
    }

    fn confirm(&mut self, label: &str, default: bool) -> Result<bool> {
        Ok(Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(label)
            .default(default)
            .interact()?)
    }

    fn select(&mut self, label: &str, options: &[&str], default: &str) -> Result<String> {
        let index = options.iter().position(|o| *o == default).unwrap_or(0);
        let chosen = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(label)
            .items(options)
            .default(index)
            .interact()?;
        Ok(options[chosen].to_string())
    }

    fn multi_select(
        &mut self,
        label: &str,
        options: &[&str],
        defaults: &[&str],
    ) -> Result<Vec<String>> {
        if options.is_empty() {
            let input: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("{label} (comma-separated)"))
                .allow_empty(true)
                .interact_text()?;
            if input.trim().is_empty() {
                return Ok(defaults.iter().map(|s| s.to_string()).collect());
            }
            return Ok(dedup(split_csv(&input)));
        }
        let checked: Vec<bool> = options.iter().map(|o| defaults.contains(o)).collect();
        let selected = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt(label)
            .items(options)
            .defaults(&checked)
            .interact()?;
        if selected.is_empty() {
            return Ok(defaults.iter().map(|s| s.to_string()).collect());
        }
        Ok(dedup(
            selected.iter().map(|&i| options[i].to_string()).collect(),
        ))
    }
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
