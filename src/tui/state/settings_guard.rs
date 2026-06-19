use crate::engine::config::Config;
use crate::engine::document::DocType;
use crate::engine::store::{Filter, Store};

/// A load-bearing `[[types]]` field that changed in the settings buffer on a
/// type that already has documents on disk. Changing `dir`/`prefix`/`store`
/// only rewrites config -- the settings screen never moves files -- so each
/// impact records what changed and how many existing docs it orphans.
pub struct TypeFieldImpact {
    pub type_name: String,
    pub field: &'static str,
    pub old: String,
    pub new: String,
    pub affected_count: usize,
}

/// Diff the dirty settings `buffer` against the `on_disk` config and report
/// every load-bearing type-field change that would orphan existing documents.
/// Pure: reads config + store, touches no `App` and no terminal. A buffer type
/// with no on-disk match is newly added (no docs yet) and is skipped; a change
/// affecting zero docs produces no impact.
pub fn detect_type_field_impacts(
    buffer: &Config,
    on_disk: &Config,
    store: &Store,
) -> Vec<TypeFieldImpact> {
    let mut impacts = Vec::new();

    for bt in &buffer.documents.types {
        let Some(dt) = on_disk.documents.types.iter().find(|d| d.name == bt.name) else {
            continue;
        };

        let affected_count = store
            .list(&Filter {
                doc_type: Some(DocType::new(&dt.name)),
                ..Default::default()
            })
            .len();
        if affected_count == 0 {
            continue;
        }

        if bt.dir != dt.dir {
            impacts.push(TypeFieldImpact {
                type_name: dt.name.clone(),
                field: "dir",
                old: dt.dir.clone(),
                new: bt.dir.clone(),
                affected_count,
            });
        }
        if bt.prefix != dt.prefix {
            impacts.push(TypeFieldImpact {
                type_name: dt.name.clone(),
                field: "prefix",
                old: dt.prefix.clone(),
                new: bt.prefix.clone(),
                affected_count,
            });
        }
        if bt.store != dt.store {
            impacts.push(TypeFieldImpact {
                type_name: dt.name.clone(),
                field: "store",
                old: dt.store.to_string(),
                new: bt.store.to_string(),
                affected_count,
            });
        }
    }

    impacts
}

/// Plain-language consequence line for one impact: names the field with its old
/// and new values, states how many docs are affected, and spells out that files
/// are not moved. Factored out as a pure builder so the wording is unit-tested
/// independently of the confirm overlay that will render it.
pub fn impact_consequence(impact: &TypeFieldImpact) -> String {
    let n = impact.affected_count;
    match impact.field {
        "dir" => format!(
            "{n} documents in {old} will no longer be found; files are not moved (dir: {old} -> {new})",
            old = impact.old,
            new = impact.new,
        ),
        "prefix" => format!(
            "{n} {name} documents use prefix {old}; they will no longer be found after changing it to {new}; files are not moved",
            name = impact.type_name,
            old = impact.old,
            new = impact.new,
        ),
        _ => format!(
            "{n} {name} documents now point at a different backend ({old} -> {new}); files are not moved",
            name = impact.type_name,
            old = impact.old,
            new = impact.new,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, StoreBackend};
    use crate::engine::fs::RealFileSystem;
    use tempfile::TempDir;

    fn doc_md(title: &str, ty: &str) -> String {
        format!(
            concat!(
                "---\n",
                "title: \"{title}\"\n",
                "type: {ty}\n",
                "status: draft\n",
                "author: \"test\"\n",
                "date: 2026-01-01\n",
                "tags: []\n",
                "---\n",
                "Body.\n",
            ),
            title = title,
            ty = ty,
        )
    }

    /// Build a real on-disk Store under a TempDir: writes `count` docs of `ty`
    /// (named by `prefix`/`dir` from the type's def in `config`) and loads them
    /// against `config` so docs are bound to the on-disk type, mirroring how the
    /// running app loads its store.
    fn populated_store(config: &Config, docs: &[(&str, usize)]) -> (TempDir, Store) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        for (ty_name, count) in docs {
            let td = config
                .documents
                .types
                .iter()
                .find(|t| &t.name == ty_name)
                .expect("type in config");
            let dir = root.join(&td.dir);
            std::fs::create_dir_all(&dir).unwrap();
            for i in 1..=*count {
                let file = dir.join(format!("{}-{:03}-doc.md", td.prefix, i));
                std::fs::write(&file, doc_md(&format!("{ty_name} {i}"), ty_name)).unwrap();
            }
        }
        let store = Store::load_with_fs(root, config, &RealFileSystem, None).unwrap();
        (tmp, store)
    }

    fn type_dir<'a>(config: &'a mut Config, name: &str) -> &'a mut String {
        &mut config
            .documents
            .types
            .iter_mut()
            .find(|t| t.name == name)
            .unwrap()
            .dir
    }

    fn type_prefix<'a>(config: &'a mut Config, name: &str) -> &'a mut String {
        &mut config
            .documents
            .types
            .iter_mut()
            .find(|t| t.name == name)
            .unwrap()
            .prefix
    }

    #[test]
    fn ac1_dir_change_on_type_with_docs_produces_one_impact() {
        let on_disk = Config::default();
        let (_tmp, store) = populated_store(&on_disk, &[("rfc", 3)]);

        let mut buffer = on_disk.clone();
        *type_dir(&mut buffer, "rfc") = "docs/proposals".to_string();

        let impacts = detect_type_field_impacts(&buffer, &on_disk, &store);

        assert_eq!(impacts.len(), 1);
        let i = &impacts[0];
        assert_eq!(i.type_name, "rfc");
        assert_eq!(i.field, "dir");
        assert_eq!(i.old, "docs/rfcs");
        assert_eq!(i.new, "docs/proposals");
        assert_eq!(i.affected_count, 3);
    }

    #[test]
    fn ac5a_non_load_bearing_change_produces_no_impact() {
        let on_disk = Config::default();
        let (_tmp, store) = populated_store(&on_disk, &[("rfc", 3)]);

        let mut buffer = on_disk.clone();
        let rfc = buffer
            .documents
            .types
            .iter_mut()
            .find(|t| t.name == "rfc")
            .unwrap();
        rfc.icon = Some("★".to_string());
        rfc.plural = "requests".to_string();

        let impacts = detect_type_field_impacts(&buffer, &on_disk, &store);
        assert!(impacts.is_empty());
    }

    #[test]
    fn ac5b_load_bearing_change_on_type_with_zero_docs_produces_no_impact() {
        let on_disk = Config::default();
        // story has zero docs on disk (only rfc docs written).
        let (_tmp, store) = populated_store(&on_disk, &[("rfc", 3)]);

        let mut buffer = on_disk.clone();
        *type_dir(&mut buffer, "story") = "docs/tickets".to_string();

        let impacts = detect_type_field_impacts(&buffer, &on_disk, &store);
        assert!(impacts.is_empty());
    }

    #[test]
    fn ac6_two_types_changed_yields_two_impacts_with_correct_counts() {
        let on_disk = Config::default();
        let (_tmp, store) = populated_store(&on_disk, &[("rfc", 12), ("story", 5)]);

        let mut buffer = on_disk.clone();
        *type_dir(&mut buffer, "rfc") = "docs/proposals".to_string();
        *type_prefix(&mut buffer, "story") = "TICKET".to_string();

        let impacts = detect_type_field_impacts(&buffer, &on_disk, &store);
        assert_eq!(impacts.len(), 2);

        let rfc = impacts.iter().find(|i| i.type_name == "rfc").unwrap();
        assert_eq!(rfc.field, "dir");
        assert_eq!(rfc.affected_count, 12);

        let story = impacts.iter().find(|i| i.type_name == "story").unwrap();
        assert_eq!(story.field, "prefix");
        assert_eq!(story.affected_count, 5);
    }

    #[test]
    fn ac2_consequence_for_dir_change_includes_old_new_count_and_not_moved() {
        let impact = TypeFieldImpact {
            type_name: "rfc".to_string(),
            field: "dir",
            old: "docs/rfcs".to_string(),
            new: "docs/proposals".to_string(),
            affected_count: 12,
        };
        let s = impact_consequence(&impact);
        assert!(s.contains("docs/rfcs"), "{s}");
        assert!(s.contains("docs/proposals"), "{s}");
        assert!(s.contains("12"), "{s}");
        assert!(s.contains("not moved"), "{s}");
    }

    #[test]
    fn ac2_consequence_for_store_change_includes_backend_strings() {
        let impact = TypeFieldImpact {
            type_name: "rfc".to_string(),
            field: "store",
            old: StoreBackend::Filesystem.to_string(),
            new: StoreBackend::GithubIssues.to_string(),
            affected_count: 7,
        };
        let s = impact_consequence(&impact);
        assert!(s.contains("filesystem"), "{s}");
        assert!(s.contains("github-issues"), "{s}");
        assert!(s.contains("7"), "{s}");
        assert!(s.contains("not moved"), "{s}");
    }
}
