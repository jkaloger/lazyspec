use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine::config::{NumberingStrategy, SqidsConfig};

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

pub fn next_sqids_id(dir: &Path, prefix: &str, sqids_config: &SqidsConfig) -> String {
    let alphabet = shuffle_alphabet(&sqids_config.salt);
    let sqids = sqids::Sqids::builder()
        .alphabet(alphabet)
        .min_length(sqids_config.min_length)
        .blocklist(HashSet::new())
        .build()
        .expect("valid sqids config");

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs();
    let mut input = ts;

    loop {
        let id = sqids.encode(&[input]).expect("sqids encode");
        let id = id.to_lowercase();
        let candidate_prefix = format!("{}-{}", prefix, id);
        if !file_exists_with_prefix(dir, &candidate_prefix) {
            return id;
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
) -> String {
    let slug = slugify(title);
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let type_upper = doc_type.to_uppercase();

    let mut filename = pattern.to_string();
    filename = filename.replace("{type}", &type_upper);
    filename = filename.replace("{title}", &slug);
    filename = filename.replace("{date}", &date);

    let has_number_placeholder = filename.contains("{n:03}") || filename.contains("{n}");
    if !has_number_placeholder {
        return filename;
    }

    if let Some(id) = pre_computed_id {
        filename = filename.replace("{n:03}", id);
        filename = filename.replace("{n}", id);
    } else {
        match numbering {
            Some((NumberingStrategy::Sqids, sqids_config)) => {
                let id = next_sqids_id(dir, &type_upper, sqids_config);
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

    filename
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
        let id = next_sqids_id(dir.path(), "RFC", &config);
        assert_eq!(id, id.to_lowercase(), "sqids ID should be lowercase");
    }

    #[test]
    fn sqids_min_length_respected() {
        let dir = TempDir::new().unwrap();
        let config = SqidsConfig {
            salt: "test-salt".to_string(),
            min_length: 6,
        };
        let id = next_sqids_id(dir.path(), "RFC", &config);
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
        let id_a = next_sqids_id(dir.path(), "RFC", &config_a);
        let id_b = next_sqids_id(dir.path(), "RFC", &config_b);
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
        let first_id = next_sqids_id(dir.path(), "RFC", &config);

        // Plant a file that matches the first ID to force a collision
        let colliding_filename = format!("RFC-{}-something.md", first_id);
        fs::write(dir.path().join(&colliding_filename), "").unwrap();

        // The second call uses the current timestamp, hits the collision,
        // and the retry loop increments the input to produce a different ID
        let second_id = next_sqids_id(dir.path(), "RFC", &config);
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
        let first_id = next_sqids_id(dir.path(), "RFC", &config);

        // Create a file that collides with the first candidate
        let colliding = format!("RFC-{}-blocker.md", first_id);
        fs::write(dir.path().join(&colliding), "").unwrap();

        // The next call uses the same timestamp, hits the collision,
        // increments input, and returns a different ID
        let second_id = next_sqids_id(dir.path(), "RFC", &config);
        assert_ne!(
            first_id, second_id,
            "should skip colliding ID and use next"
        );
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
        );
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
        );
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
        );
        assert!(
            filename.starts_with("RFC-001-"),
            "explicit incremental should use numbers, got: {}",
            filename
        );
    }
}
