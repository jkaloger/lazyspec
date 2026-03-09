use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::Result;

pub fn build_create_children_prompt(doc_content: &str, child_type: &str) -> String {
    format!(
        "You are a specification document generator. Given the parent document below, generate \
child documents of type \"{child_type}\" that break down the parent into actionable pieces. \
For each child document, run `lazyspec create {child_type}` with an appropriate title \
and fill in the generated file with relevant content derived from the parent. \
Preserve traceability by including a relation back to the parent document.\n\n\
---\n\n{doc_content}"
    )
}

pub fn build_expand_prompt(doc_content: &str) -> String {
    format!(
        "You are editing a specification document. Your task is to flesh out and expand any sparse \
or incomplete sections while preserving the YAML frontmatter exactly as-is. Do not remove \
or reorder existing content. Focus on adding detail, clarifying intent, and filling gaps. \
Output the complete updated document.\n\n---\n\n{}",
        doc_content
    )
}

pub struct AgentSpawner {
    children: Vec<Child>,
}

impl AgentSpawner {
    pub fn new() -> Self {
        AgentSpawner {
            children: Vec::new(),
        }
    }

    pub fn spawn(&mut self, prompt: &str, doc_path: &Path) -> Result<()> {
        let child = Command::new("claude")
            .args(["-p", prompt])
            .arg(doc_path)
            .args(["--allowedTools", "Read,Edit,Bash(lazyspec *)"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        self.children.push(child);
        Ok(())
    }

    pub fn poll_finished(&mut self) {
        self.children.retain_mut(|child| {
            match child.try_wait() {
                Ok(Some(_status)) => false,
                Ok(None) => true,
                Err(_) => false,
            }
        });
    }

    pub fn active_count(&self) -> usize {
        self.children.len()
    }
}
