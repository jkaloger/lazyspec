use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result as AnyResult};

#[cfg(test)]
use crate::engine::config::StoreBackend;
use crate::engine::config::{NumberingStrategy, SqidsConfig, TypeDef};

/// Whether a type's template is a single file or a directory of parts
/// (RFC-074).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    File,
    Directory,
}

/// Resolve whether `type_def`'s template is a file or a directory, checking
/// `templates_dir` (the configured templates directory, already resolved
/// against the project/docs root). A directory template at
/// `templates_dir/{type}/` takes precedence over a `{type}.md` file; it must
/// declare `index.md`, and the type must declare `subdirectory = true` --
/// either violation is a config error (RFC-074 AC1).
pub fn resolve_template_kind(templates_dir: &Path, type_def: &TypeDef) -> AnyResult<TemplateKind> {
    let dir_template = templates_dir.join(&type_def.name);
    if !dir_template.is_dir() {
        return Ok(TemplateKind::File);
    }
    if !type_def.subdirectory {
        bail!(
            "type '{}' has a directory template at {}, but subdirectory = false; a directory \
             template scaffolds a folder of parts, so declare subdirectory = true for it",
            type_def.name,
            dir_template.display()
        );
    }
    if !dir_template.join("index.md").is_file() {
        bail!(
            "template directory {} has no index.md; a directory template's index.md is the \
             parent document's template and is required",
            dir_template.display()
        );
    }
    Ok(TemplateKind::Directory)
}

/// The declared part names (file stems, `.md` extension stripped) in a
/// directory template, in template order (sorted by stem), excluding
/// `index.md`. Empty when the type's template is not a directory (or the
/// directory cannot be read). Stems, not filenames, because the loader
/// (`scan_document_folder`, src/engine/store/loader.rs) matches these against
/// a part's stem, not its filename -- used to order a bundle's parts:
/// declared parts first in this order, then any extra parts alphabetically
/// (RFC-074 AC5). Restricted to `.md` files, mirroring the AC3 part/sidecar
/// split, so a non-`.md` template file (a sidecar) never masquerades as a
/// declared part. Deliberately lenient about a misconfigured template
/// (missing `index.md`, etc.) -- that is reported by [`resolve_template_kind`],
/// not by ordering, which the loader consults for every document regardless
/// of whether the template is valid.
pub fn directory_template_part_order(templates_dir: &Path, type_name: &str) -> Vec<String> {
    let dir_template = templates_dir.join(type_name);
    let Ok(entries) = fs::read_dir(&dir_template) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .filter(|n| n != "index")
        .collect();
    names.sort();
    names
}

pub fn render_template(template_content: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template_content.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn next_number(dir: &Path, prefix: &str) -> u32 {
    let mut max = 0u32;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix) {
                if let Some(rest) = name.strip_prefix(prefix) {
                    let rest = rest.trim_start_matches('-');
                    let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(n) = num_str.parse::<u32>() {
                        max = max.max(n);
                    }
                }
            }
        }
    }
    max + 1
}

pub fn shuffle_alphabet(salt: &str) -> Vec<char> {
    let mut alphabet: Vec<char> = sqids::DEFAULT_ALPHABET.chars().collect();
    if salt.is_empty() {
        return alphabet;
    }
    let salt_bytes = salt.as_bytes();
    let len = alphabet.len();
    for i in (1..len).rev() {
        let salt_idx = (len - 1 - i) % salt_bytes.len();
        let j = (salt_bytes[salt_idx] as usize + salt_idx + i) % (i + 1);
        alphabet.swap(i, j);
    }
    alphabet
}

fn file_exists_with_prefix(dir: &Path, prefix: &str) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(prefix) {
                return true;
            }
        }
    }
    false
}

pub fn next_sqids_id(
    dir: &Path,
    prefix: &str,
    sqids_config: &SqidsConfig,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let alphabet = shuffle_alphabet(&sqids_config.salt);
    let sqids = sqids::Sqids::builder()
        .alphabet(alphabet)
        .min_length(sqids_config.min_length)
        .blocklist(HashSet::new())
        .build()?;

    let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let mut input = ts;

    loop {
        let id = sqids.encode(&[input])?.to_lowercase();
        let candidate_prefix = format!("{}-{}", prefix, id);
        if !file_exists_with_prefix(dir, &candidate_prefix) {
            return Ok(id);
        }
        input += 1;
    }
}

pub fn resolve_filename(
    pattern: &str,
    doc_type: &str,
    title: &str,
    dir: &Path,
    numbering: Option<(&NumberingStrategy, &SqidsConfig)>,
    pre_computed_id: Option<&str>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let slug = slugify(title);
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let type_upper = doc_type.to_uppercase();

    let mut filename = pattern.to_string();
    filename = filename.replace("{type}", &type_upper);
    filename = filename.replace("{title}", &slug);
    filename = filename.replace("{date}", &date);

    let has_number_placeholder = filename.contains("{n:03}") || filename.contains("{n}");
    if !has_number_placeholder {
        return Ok(filename);
    }

    if let Some(id) = pre_computed_id {
        filename = filename.replace("{n:03}", id);
        filename = filename.replace("{n}", id);
    } else {
        match numbering {
            Some((NumberingStrategy::Sqids, sqids_config)) => {
                let id = next_sqids_id(dir, &type_upper, sqids_config)?;
                filename = filename.replace("{n:03}", &id);
                filename = filename.replace("{n}", &id);
            }
            _ => {
                let n = next_number(dir, &type_upper);
                if filename.contains("{n:03}") {
                    filename = filename.replace("{n:03}", &format!("{:03}", n));
                } else if filename.contains("{n}") {
                    filename = filename.replace("{n}", &n.to_string());
                }
            }
        }
    }

    Ok(filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn sqids_id_is_lowercase() {
        let dir = TempDir::new().unwrap();
        let config = SqidsConfig {
            salt: "test-salt".to_string(),
            min_length: 3,
        };
        let id = next_sqids_id(dir.path(), "RFC", &config).unwrap();
        assert_eq!(id, id.to_lowercase(), "sqids ID should be lowercase");
    }

    #[test]
    fn sqids_min_length_respected() {
        let dir = TempDir::new().unwrap();
        let config = SqidsConfig {
            salt: "test-salt".to_string(),
            min_length: 6,
        };
        let id = next_sqids_id(dir.path(), "RFC", &config).unwrap();
        assert!(
            id.len() >= 6,
            "expected min_length 6, got {} (id: {})",
            id.len(),
            id
        );
    }

    #[test]
    fn sqids_salt_changes_output() {
        let dir = TempDir::new().unwrap();
        let config_a = SqidsConfig {
            salt: "salt-alpha".to_string(),
            min_length: 3,
        };
        let config_b = SqidsConfig {
            salt: "salt-beta".to_string(),
            min_length: 3,
        };
        let id_a = next_sqids_id(dir.path(), "RFC", &config_a).unwrap();
        let id_b = next_sqids_id(dir.path(), "RFC", &config_b).unwrap();
        assert_ne!(id_a, id_b, "different salts should produce different IDs");
    }

    #[test]
    fn sqids_collision_retry() {
        let dir = TempDir::new().unwrap();
        let config = SqidsConfig {
            salt: "collision-test".to_string(),
            min_length: 3,
        };

        // Generate the first ID (derived from the current Unix timestamp)
        let first_id = next_sqids_id(dir.path(), "RFC", &config).unwrap();

        // Plant a file that matches the first ID to force a collision
        let colliding_filename = format!("RFC-{}-something.md", first_id);
        fs::write(dir.path().join(&colliding_filename), "").unwrap();

        // The second call uses the current timestamp, hits the collision,
        // and the retry loop increments the input to produce a different ID
        let second_id = next_sqids_id(dir.path(), "RFC", &config).unwrap();
        assert_ne!(first_id, second_id, "should retry on collision");
    }

    #[test]
    fn sqids_collision_retry_forced() {
        let dir = TempDir::new().unwrap();
        let config = SqidsConfig {
            salt: "forced-collision".to_string(),
            min_length: 3,
        };

        // Generate the first ID (based on current timestamp)
        let first_id = next_sqids_id(dir.path(), "RFC", &config).unwrap();

        // Create a file that collides with the first candidate
        let colliding = format!("RFC-{}-blocker.md", first_id);
        fs::write(dir.path().join(&colliding), "").unwrap();

        // The next call uses the same timestamp, hits the collision,
        // increments input, and returns a different ID
        let second_id = next_sqids_id(dir.path(), "RFC", &config).unwrap();
        assert_ne!(first_id, second_id, "should skip colliding ID and use next");
    }

    #[test]
    fn resolve_filename_with_sqids() {
        let dir = TempDir::new().unwrap();
        let config = SqidsConfig {
            salt: "resolve-test".to_string(),
            min_length: 3,
        };
        let filename = resolve_filename(
            "{type}-{n:03}-{title}.md",
            "rfc",
            "My Feature",
            dir.path(),
            Some((&NumberingStrategy::Sqids, &config)),
            None,
        )
        .unwrap();
        assert!(filename.starts_with("RFC-"), "got: {}", filename);
        assert!(filename.ends_with("-my-feature.md"), "got: {}", filename);
        // The middle part should be the sqids ID, not zero-padded
        let parts: Vec<&str> = filename.split('-').collect();
        assert!(
            !parts[1].chars().all(|c| c.is_ascii_digit()),
            "sqids ID should not be purely numeric, got: {}",
            parts[1]
        );
    }

    #[test]
    fn resolve_filename_incremental_unchanged() {
        let dir = TempDir::new().unwrap();
        let filename = resolve_filename(
            "{type}-{n:03}-{title}.md",
            "rfc",
            "Test",
            dir.path(),
            None,
            None,
        )
        .unwrap();
        assert!(
            filename.starts_with("RFC-001-"),
            "incremental should still work, got: {}",
            filename
        );
    }

    #[test]
    fn resolve_filename_explicit_incremental() {
        let dir = TempDir::new().unwrap();
        let config = SqidsConfig {
            salt: "unused".to_string(),
            min_length: 3,
        };
        let filename = resolve_filename(
            "{type}-{n:03}-{title}.md",
            "rfc",
            "Test",
            dir.path(),
            Some((&NumberingStrategy::Incremental, &config)),
            None,
        )
        .unwrap();
        assert!(
            filename.starts_with("RFC-001-"),
            "explicit incremental should use numbers, got: {}",
            filename
        );
    }

    // RFC-074 AC1: no `{type}/` directory in the templates dir -> a file template.
    #[test]
    fn resolve_template_kind_is_file_when_no_directory_template_exists() {
        let dir = TempDir::new().unwrap();
        let type_def = TypeDef::test_fixture("change", StoreBackend::Filesystem);

        let kind = resolve_template_kind(dir.path(), &type_def).unwrap();

        assert_eq!(kind, TemplateKind::File);
    }

    // RFC-074 AC1: a `{type}/` directory with `index.md` and `subdirectory = true`
    // resolves to a directory template.
    #[test]
    fn resolve_template_kind_is_directory_when_index_md_present_and_subdirectory_true() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("change")).unwrap();
        fs::write(dir.path().join("change/index.md"), "index").unwrap();
        let type_def = TypeDef {
            subdirectory: true,
            ..TypeDef::test_fixture("change", StoreBackend::Filesystem)
        };

        let kind = resolve_template_kind(dir.path(), &type_def).unwrap();

        assert_eq!(kind, TemplateKind::Directory);
    }

    // RFC-074 AC1: a directory template with no `index.md` is a config error.
    #[test]
    fn resolve_template_kind_errors_when_directory_template_has_no_index_md() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("change")).unwrap();
        fs::write(dir.path().join("change/design.md"), "design").unwrap();
        let type_def = TypeDef {
            subdirectory: true,
            ..TypeDef::test_fixture("change", StoreBackend::Filesystem)
        };

        let err = resolve_template_kind(dir.path(), &type_def).unwrap_err();

        assert!(err.to_string().contains("index.md"), "got: {err}");
    }

    // RFC-074 AC1: `subdirectory = false` on a type whose template is a
    // directory is a config error.
    #[test]
    fn resolve_template_kind_errors_when_subdirectory_false_for_directory_template() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("change")).unwrap();
        fs::write(dir.path().join("change/index.md"), "index").unwrap();
        let type_def = TypeDef {
            subdirectory: false,
            ..TypeDef::test_fixture("change", StoreBackend::Filesystem)
        };

        let err = resolve_template_kind(dir.path(), &type_def).unwrap_err();

        assert!(err.to_string().contains("subdirectory"), "got: {err}");
    }

    // RFC-074 AC5: declared part order is the template directory's file
    // stems sorted, excluding index.md -- stems, not filenames, since the
    // loader matches these against a part's stem (src/engine/store/loader.rs).
    #[test]
    fn directory_template_part_order_is_sorted_excluding_index() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("change")).unwrap();
        for name in ["index.md", "tasks.md", "arch.md", "design.md"] {
            fs::write(dir.path().join("change").join(name), "x").unwrap();
        }

        let order = directory_template_part_order(dir.path(), "change");

        assert_eq!(order, vec!["arch", "design", "tasks"]);
    }

    // RFC-074 AC5: a non-`.md` template file (a sidecar) never counts as a
    // declared part.
    #[test]
    fn directory_template_part_order_excludes_non_md_files() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("change")).unwrap();
        fs::write(dir.path().join("change/index.md"), "x").unwrap();
        fs::write(dir.path().join("change/design.md"), "x").unwrap();
        fs::write(dir.path().join("change/notes.yaml"), "x").unwrap();

        let order = directory_template_part_order(dir.path(), "change");

        assert_eq!(order, vec!["design"]);
    }

    // No directory template at all -> no declared order, not an error.
    #[test]
    fn directory_template_part_order_is_empty_when_no_directory_template() {
        let dir = TempDir::new().unwrap();

        let order = directory_template_part_order(dir.path(), "change");

        assert!(order.is_empty());
    }
}
