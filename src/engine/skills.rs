use std::path::PathBuf;

/// The default generic verb skill set, embedded at build time from the
/// canonical on-disk source at `skills/` (repo root). Mirrors
/// `fs_ops::default_template`'s "binary carries the default" intent so
/// `skills install` works in any project regardless of an on-disk source. The
/// router skill is embedded under the stable key `lazy/SKILL.md`; install
/// renames its directory to the configured entry.
const EMBEDDED_SKILLS: &[(&str, &str)] = &[
    (
        "scaffold/SKILL.md",
        include_str!("../../skills/scaffold/SKILL.md"),
    ),
    (
        "co-write/SKILL.md",
        include_str!("../../skills/co-write/SKILL.md"),
    ),
    (
        "generate/SKILL.md",
        include_str!("../../skills/generate/SKILL.md"),
    ),
    (
        "advance/SKILL.md",
        include_str!("../../skills/advance/SKILL.md"),
    ),
    (
        "execute/SKILL.md",
        include_str!("../../skills/execute/SKILL.md"),
    ),
    (
        "orchestrate/SKILL.md",
        include_str!("../../skills/orchestrate/SKILL.md"),
    ),
    (
        "review/SKILL.md",
        include_str!("../../skills/review/SKILL.md"),
    ),
    (
        "review-work/SKILL.md",
        include_str!("../../skills/review-work/SKILL.md"),
    ),
    (
        "systematic-debugging/SKILL.md",
        include_str!("../../skills/systematic-debugging/SKILL.md"),
    ),
    ("lazy/SKILL.md", include_str!("../../skills/lazy/SKILL.md")),
];

/// The stable embedded key for the router skill. Install renames this
/// directory to the configured `[skills] entry`.
pub const ROUTER_KEY: &str = "lazy/SKILL.md";

/// Iterates the embedded skill set as `(relative path under the set root, file
/// contents)`. Callers stay agnostic to the embedding mechanism.
pub fn embedded_skill_set() -> impl Iterator<Item = (PathBuf, &'static str)> {
    EMBEDDED_SKILLS
        .iter()
        .map(|(path, contents)| (PathBuf::from(path), *contents))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn embedded_skill_set_is_non_empty() {
        assert!(embedded_skill_set().next().is_some());
    }

    #[test]
    fn embedded_skill_set_contains_router() {
        let has_router = embedded_skill_set().any(|(path, _)| path == Path::new(ROUTER_KEY));
        assert!(
            has_router,
            "embedded set must contain the router at {ROUTER_KEY}"
        );
    }
}
