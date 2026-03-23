use super::FixOutput;

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

    result
}
