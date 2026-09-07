use crate::engine::ops::fix::{ConfigFixResult, FixOutput, GovernsFixResult};

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
        // A failed write changed nothing, so every "Migrated"/"Dropped" line
        // below it would be a lie. Report why instead.
        if let Some(e) = &r.error {
            result.push_str(&format!(
                "error: could not update relations in {}: {}\n",
                r.path, e
            ));
            continue;
        }
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

/// One line per rotted pin. The verb comes from whether the write landed, not
/// from `dry_run`: a rewrite that could not reach the document must not read as
/// one that did, and the reason it failed is the only thing that tells the
/// reader what to do next (DICTUM-006).
pub(super) fn format_governs_human(rewrites: &[GovernsFixResult], dry_run: bool) -> String {
    rewrites
        .iter()
        .map(|r| match &r.error {
            Some(e) => format!("error: could not repin {}: {}\n", r.path, e),
            None => {
                let verb = if dry_run { "Would repin" } else { "Repinned" };
                format!("{} {}: {} -> {}\n", verb, r.path, r.old_glob, r.new_glob)
            }
        })
        .collect()
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

    // No line names an added rule, and no result field carries one. A standard
    // constraint the config was missing is declared as an `[[edges]]` row and
    // seeded as one, so the "Wrote edge" line below is the whole report of it;
    // a line calling the same name a rule would point the reader at a table
    // that no longer loads.

    for name in &result.lifecycles_added {
        if dry_run {
            out.push_str(&format!("Would add default lifecycle to type {}\n", name));
        } else {
            out.push_str(&format!("Added default lifecycle to type {}\n", name));
        }
    }

    // The edge migration rewrites rather than appends, so a run can change the
    // file while adding nothing. Reporting only the additions would let such a
    // run print "nothing to add" over a rewrite.
    for name in &result.edges_written {
        if dry_run {
            out.push_str(&format!("Would write edge {}\n", name));
        } else {
            out.push_str(&format!("Wrote edge {}\n", name));
        }
    }

    for name in &result.rules_removed {
        if dry_run {
            out.push_str(&format!("Would remove rule {}\n", name));
        } else {
            out.push_str(&format!("Removed rule {}\n", name));
        }
    }

    for name in &result.traversal_removed {
        if dry_run {
            out.push_str(&format!(
                "Would remove traversal from relationship {}\n",
                name
            ));
        } else {
            out.push_str(&format!("Removed traversal from relationship {}\n", name));
        }
    }

    // Last, so the destructions are the lines still on screen when the reader
    // stops reading. Neither is recoverable from anything else in the plan: a
    // comment leaves no trace in the migrated config, and the retired gate
    // changes no finding on either side of the rewrite.
    for lost in &result.comments_lost {
        let verb = if dry_run { "Would lose" } else { "Lost" };
        out.push_str(&format!(
            "{verb} comment on {} {}: {}\n",
            lost.block.label(),
            lost.name,
            lost.comment
        ));
    }

    for name in &result.gates_dropped {
        let verb = if dry_run { "Would drop" } else { "Dropped" };
        out.push_str(&format!(
            "{verb} the require_parent_status gate on rule {name}: \
             status-conditioned create gating is retired with no successor (ADR-033)\n"
        ));
    }

    if out.is_empty() {
        out.push_str("Config already up to date; nothing to add and nothing to migrate\n");
    }

    out
}
