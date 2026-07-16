use anyhow::{anyhow, bail, Result};

pub use crate::engine::ops::update::{run, run_with_config};

const RESERVED_ATTR_KEYS: &[&str] = &["status", "title", "body", "author"];

/// Parse repeatable `--attr key=value` flags into owned `(key, value)` pairs.
///
/// Splits on the FIRST `=` so values may themselves contain `=`. A missing `=`,
/// an empty key, or a reserved field name (which has its own dedicated flag) is
/// an error.
pub fn parse_attr_pairs(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter()
        .map(|entry| {
            let (key, value) = entry
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid --attr, expected key=value: {entry}"))?;
            if key.is_empty() {
                bail!("invalid --attr, empty key: {entry}");
            }
            if RESERVED_ATTR_KEYS.contains(&key) {
                bail!("'{key}' is a reserved field and cannot be set via --attr; use --{key}");
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use clap::Parser;

    // AC4: clap collects each --attr occurrence into a Vec.
    #[test]
    fn clap_collects_multiple_attr_flags() {
        let cli = Cli::try_parse_from([
            "lazyspec",
            "update",
            "STORY-1",
            "--attr",
            "owner=jkaloger",
            "--attr",
            "estimate=3",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Update { attr, .. }) => {
                assert_eq!(attr, vec!["owner=jkaloger", "estimate=3"]);
            }
            _ => panic!("expected Update command"),
        }
    }

    #[test]
    fn parse_attr_pairs_basic() {
        let pairs = parse_attr_pairs(&["owner=jkaloger".to_string()]).unwrap();
        assert_eq!(pairs, vec![("owner".to_string(), "jkaloger".to_string())]);
    }

    // Edge: split on the FIRST '=' so the value may contain '='.
    #[test]
    fn parse_attr_pairs_value_with_equals() {
        let pairs = parse_attr_pairs(&["k=a=b".to_string()]).unwrap();
        assert_eq!(pairs, vec![("k".to_string(), "a=b".to_string())]);
    }

    // Edge: missing '=' bails.
    #[test]
    fn parse_attr_pairs_missing_equals_bails() {
        let err = parse_attr_pairs(&["badpair".to_string()]).unwrap_err();
        assert!(err.to_string().contains("expected key=value"), "got: {err}");
    }

    // Edge: empty key bails.
    #[test]
    fn parse_attr_pairs_empty_key_bails() {
        let err = parse_attr_pairs(&["=v".to_string()]).unwrap_err();
        assert!(err.to_string().contains("empty key"), "got: {err}");
    }

    #[test]
    fn parse_attr_pairs_reserved_key_bails() {
        let err = parse_attr_pairs(&["status=done".to_string()]).unwrap_err();
        assert!(err.to_string().contains("reserved"), "got: {err}");
    }
}
