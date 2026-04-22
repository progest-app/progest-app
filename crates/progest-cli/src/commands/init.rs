//! `progest init` — bootstrap a `.progest/` layout in the current directory.

use std::path::Path;

use anyhow::{Context, Result};
use progest_core::project;

/// Run `progest init` against `cwd`, defaulting the project name to the
/// basename of `cwd` when not supplied.
pub fn run(cwd: &Path, name: Option<String>) -> Result<()> {
    let default_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();
    let name = name.unwrap_or(default_name);

    let root = project::initialize(cwd, &name)
        .with_context(|| format!("failed to initialize project at `{}`", cwd.display()))?;

    println!(
        "Initialized Progest project `{name}` at {}",
        root.root().display()
    );
    println!("  • {}", root.project_toml().display());
    println!("  • {}", root.user_ignore().display());
    println!("  • {}", root.index_db().display());
    println!("\nNext steps: drop some files in the project and run `progest scan`.");
    Ok(())
}
