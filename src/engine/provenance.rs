use crate::engine::config::Config;
use crate::engine::store_dispatch::PushOutcome;
use anyhow::{anyhow, Result};
use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub enum ProvenanceError {
    Empty,
    Duplicate(String),
}

impl fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProvenanceError::Empty => write!(f, "citation must not be empty"),
            ProvenanceError::Duplicate(s) => write!(f, "citation already present: {}", s),
        }
    }
}

impl std::error::Error for ProvenanceError {}

pub fn validate_citation(citation: &str) -> Result<&str, ProvenanceError> {
    let trimmed = citation.trim();
    if trimmed.is_empty() {
        Err(ProvenanceError::Empty)
    } else {
        Ok(trimmed)
    }
}

pub fn set_provenance(
    root: &Path,
    config: &Config,
    type_name: &str,
    doc_id: &str,
    new_list: &[String],
) -> Result<PushOutcome> {
    let type_def = config
        .type_by_name(type_name)
        .ok_or_else(|| anyhow!("unknown document type: {}", type_name))?;

    // Dispatch through the store registry: a new backend routes here by being
    // registered in `build_registry`, not by adding a match arm.
    let mut registry = crate::engine::store_dispatch::build_registry(root, config);
    registry
        .for_type(type_def)?
        .set_provenance(type_def, doc_id, new_list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_citation_rejects_empty() {
        assert!(matches!(validate_citation(""), Err(ProvenanceError::Empty)));
    }

    #[test]
    fn validate_citation_rejects_whitespace_only() {
        assert!(matches!(
            validate_citation("   "),
            Err(ProvenanceError::Empty)
        ));
    }

    #[test]
    fn validate_citation_trims_and_returns() {
        assert_eq!(validate_citation("  X  ").unwrap(), "X");
    }

    #[test]
    fn display_empty() {
        assert!(format!("{}", ProvenanceError::Empty).contains("empty"));
    }

    #[test]
    fn display_duplicate() {
        let s = format!("{}", ProvenanceError::Duplicate("foo".into()));
        assert!(s.contains("foo") && s.contains("already"));
    }
}
