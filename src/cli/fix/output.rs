use crate::engine::ops::fix::{ConfigFixResult, FixOutput};

pub(super) fn format_human(output: &FixOutput, dry_run: bool) -> String {
    let mut result = String::new();

    for r in &output.field_fixes {
        if r.fields_added.is_empty() {
            continue;
        }
        let fields = r.fields_added.join(", ");
        if dry_run {
            result.push_str(&format!("Would fix {} (would add: {})\n", r.path, fields));
        } else {
            result.push_str(&format!("Fixed {} (added: {})\n", r.path, fields));
        }
    }

    for c in &output.conflict_fixes {
        if dry_run {
            result.push_str(&format!("Would rename {} -> {}\n", c.old_path, c.new_path));
        } else {
            result.push_str(&format!("Renamed {} -> {}\n", c.old_path, c.new_path));
        }
    }

    for r in &output.status_fixes {
        if dry_run {
            result.push_str(&format!(
                "Would fix status in {}: {} -> {}\n",
                r.path, r.old_status, r.new_status
            ));
        } else {
            result.push_str(&format!(
                "Fixed status in {}: {} -> {}\n",
                r.path, r.old_status, r.new_status
            ));
        }
    }

    for r in &output.relation_fixes {
        for (old_target, new_target) in &r.replacements {
            if dry_run {
                result.push_str(&format!(
                    "Would migrate relation in {}: {} -> {}\n",
                    r.path, old_target, new_target
                ));
            } else {
                result.push_str(&format!(
                    "Migrated relation in {}: {} -> {}\n",
                    r.path, old_target, new_target
                ));
            }
        }
        for (rel_type, target) in &r.deduped {
            if dry_run {
                result.push_str(&format!(
                    "Would drop duplicate relation in {}: {} {}\n",
                    r.path, rel_type, target
                ));
            } else {
                result.push_str(&format!(
                    "Dropped duplicate relation in {}: {} {}\n",
                    r.path, rel_type, target
                ));
            }
        }
    }

    result
}

pub(super) fn format_config_human(result: &ConfigFixResult, dry_run: bool) -> String {
    let mut out = String::new();

    for name in &result.relationships_added {
        if dry_run {
            out.push_str(&format!("Would add relationship {}\n", name));
        } else {
            out.push_str(&format!("Added relationship {}\n", name));
        }
    }

    for name in &result.rules_added {
        if dry_run {
            out.push_str(&format!("Would add rule {}\n", name));
        } else {
            out.push_str(&format!("Added rule {}\n", name));
        }
    }

    for name in &result.lifecycles_added {
        if dry_run {
            out.push_str(&format!("Would add default lifecycle to type {}\n", name));
        } else {
            out.push_str(&format!("Added default lifecycle to type {}\n", name));
        }
    }

    if out.is_empty() {
        out.push_str("Config already up to date; nothing to add\n");
    }

    out
}
