use crate::engine::config::Config;
use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

pub fn run(root: &Path) -> Result<()> {
    let config_path = root.join(".lazyspec.toml");
    if config_path.exists() {
        bail!(".lazyspec.toml already exists");
    }

    let config = Config::default();

    fs::create_dir_all(root.join(&config.directories.rfcs))?;
    fs::create_dir_all(root.join(&config.directories.adrs))?;
    fs::create_dir_all(root.join(&config.directories.specs))?;
    fs::create_dir_all(root.join(&config.directories.plans))?;
    fs::create_dir_all(root.join(&config.templates.dir))?;

    fs::write(&config_path, config.to_toml()?)?;

    println!("Initialized lazyspec in {}", root.display());
    Ok(())
}
