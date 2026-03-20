use crate::engine::template;
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use std::path::Path;
use std::process::Command;

#[derive(Serialize)]
pub struct Reservation {
    pub prefix: String,
    pub number: u32,
    pub ref_path: String,
}

pub fn list_reservations(repo_root: &Path, remote: &str) -> Result<Vec<Reservation>> {
    let output = Command::new("git")
        .args(["ls-remote", "--refs", remote, "refs/reservations/*"])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lower = stderr.to_lowercase();
        if lower.contains("could not read")
            || lower.contains("fatal:")
            || lower.contains("connection")
            || lower.contains("timeout")
            || lower.contains("auth")
            || lower.contains("resolve host")
        {
            bail!(
                "Remote '{}' is unreachable: {}",
                remote,
                stderr.trim()
            );
        }
        bail!("git ls-remote failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let reservations = stdout
        .lines()
        .filter_map(|line| {
            let refname = line.split_whitespace().nth(1)?;
            let rest = refname.strip_prefix("refs/reservations/")?;
            let (prefix, num_str) = rest.rsplit_once('/')?;
            let number = num_str.parse::<u32>().ok()?;
            Some(Reservation {
                prefix: prefix.to_string(),
                number,
                ref_path: refname.to_string(),
            })
        })
        .collect();

    Ok(reservations)
}

pub fn delete_remote_ref(repo_root: &Path, remote: &str, ref_path: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["push", remote, "--delete", ref_path])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git push --delete failed: {}", stderr.trim());
    }

    Ok(())
}

fn ls_remote(repo_root: &Path, remote: &str, prefix: &str) -> Result<Vec<u32>> {
    let pattern = format!("refs/reservations/{prefix}/*");
    let output = Command::new("git")
        .args(["ls-remote", "--refs", remote, &pattern])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lower = stderr.to_lowercase();
        if lower.contains("could not read")
            || lower.contains("fatal:")
            || lower.contains("connection")
            || lower.contains("timeout")
            || lower.contains("auth")
            || lower.contains("resolve host")
        {
            bail!(
                "Remote '{}' is unreachable: {}\n\
                 Hint: use --numbering incremental or --numbering sqids as an override",
                remote,
                stderr.trim()
            );
        }
        bail!("git ls-remote failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ref_prefix = format!("refs/reservations/{prefix}/");

    let numbers: Vec<u32> = stdout
        .lines()
        .filter_map(|line| {
            let refname = line.split_whitespace().nth(1)?;
            let suffix = refname.strip_prefix(&ref_prefix)?;
            suffix.parse::<u32>().ok()
        })
        .collect();

    Ok(numbers)
}

fn create_local_ref(repo_root: &Path, prefix: &str, num: u32) -> Result<()> {
    let hash_output = Command::new("git")
        .args(["hash-object", "-w", "-t", "blob", "--stdin"])
        .stdin(std::process::Stdio::null())
        .current_dir(repo_root)
        .output()?;

    if !hash_output.status.success() {
        let stderr = String::from_utf8_lossy(&hash_output.stderr);
        bail!("git hash-object failed: {}", stderr.trim());
    }

    let sha = String::from_utf8_lossy(&hash_output.stdout).trim().to_string();
    let refname = format!("refs/reservations/{prefix}/{num}");

    let update_output = Command::new("git")
        .args(["update-ref", &refname, &sha])
        .current_dir(repo_root)
        .output()?;

    if !update_output.status.success() {
        let stderr = String::from_utf8_lossy(&update_output.stderr);
        bail!("git update-ref failed: {}", stderr.trim());
    }

    Ok(())
}

fn push_ref(repo_root: &Path, remote: &str, prefix: &str, num: u32) -> Result<bool> {
    let refname = format!("refs/reservations/{prefix}/{num}");
    let output = Command::new("git")
        .args(["push", remote, &refname])
        .current_dir(repo_root)
        .output()?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lower = stderr.to_lowercase();
    if lower.contains("rejected") || lower.contains("already exists") || lower.contains("non-fast-forward") {
        return Ok(false);
    }

    Err(anyhow!("git push failed: {}", stderr.trim()))
}

fn cleanup_local_ref(repo_root: &Path, prefix: &str, num: u32) -> Result<()> {
    let refname = format!("refs/reservations/{prefix}/{num}");
    let output = Command::new("git")
        .args(["update-ref", "-d", &refname])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git update-ref -d failed: {}", stderr.trim());
    }

    Ok(())
}

pub fn reserve_next(
    repo_root: &Path,
    remote: &str,
    prefix: &str,
    max_retries: u8,
    docs_dir: &Path,
) -> Result<u32> {
    let remote_existing = ls_remote(repo_root, remote, prefix)?;
    let remote_max = remote_existing.iter().copied().max().unwrap_or(0);
    let local_max = template::next_number(docs_dir, prefix).saturating_sub(1);
    let base = remote_max.max(local_max);
    let mut candidate = base + 1;

    for attempt in 0..max_retries {
        create_local_ref(repo_root, prefix, candidate)?;

        match push_ref(repo_root, remote, prefix, candidate) {
            Ok(true) => return Ok(candidate),
            Ok(false) => {
                cleanup_local_ref(repo_root, prefix, candidate)?;
                candidate += 1;
            }
            Err(e) => {
                let _ = cleanup_local_ref(repo_root, prefix, candidate);
                return Err(e.context(format!(
                    "Push failed on attempt {} of {}",
                    attempt + 1,
                    max_retries
                )));
            }
        }
    }

    bail!(
        "Failed to reserve a document number for prefix '{}' after {} attempts \
         (tried numbers {} through {})",
        prefix,
        max_retries,
        base + 1,
        base + max_retries as u32
    )
}
