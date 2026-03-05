#![allow(dead_code)]

use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct TestFixture {
    pub dir: TempDir,
}

impl TestFixture {
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs/rfcs")).unwrap();
        std::fs::create_dir_all(root.join("docs/adrs")).unwrap();
        std::fs::create_dir_all(root.join("docs/stories")).unwrap();
        std::fs::create_dir_all(root.join("docs/iterations")).unwrap();
        Self { dir }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn config(&self) -> Config {
        Config::default()
    }

    pub fn store(&self) -> Store {
        Store::load(self.root(), &self.config()).unwrap()
    }

    pub fn write_doc(&self, rel_path: &str, content: &str) -> PathBuf {
        let path = self.root().join(rel_path);
        std::fs::write(&path, content).unwrap();
        path
    }

    pub fn write_rfc(&self, filename: &str, title: &str, status: &str) -> PathBuf {
        let content = format!(
            "---\ntitle: \"{}\"\ntype: rfc\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
            title, status
        );
        self.write_doc(&format!("docs/rfcs/{}", filename), &content)
    }

    pub fn write_story(
        &self,
        filename: &str,
        title: &str,
        status: &str,
        implements: Option<&str>,
    ) -> PathBuf {
        let related = match implements {
            Some(path) => format!("related:\n- implements: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: story\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/stories/{}", filename), &content)
    }

    pub fn write_iteration(
        &self,
        filename: &str,
        title: &str,
        status: &str,
        implements: Option<&str>,
    ) -> PathBuf {
        let related = match implements {
            Some(path) => format!("related:\n- implements: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: iteration\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/iterations/{}", filename), &content)
    }

    pub fn write_adr(
        &self,
        filename: &str,
        title: &str,
        status: &str,
        related_to: Option<&str>,
    ) -> PathBuf {
        let related = match related_to {
            Some(path) => format!("related:\n- related to: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: adr\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/adrs/{}", filename), &content)
    }
}
