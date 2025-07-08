//! Generate the environment variables reference from `ty_static::EnvVars`.

use std::collections::BTreeSet;
use std::path::PathBuf;
use ty_static::EnvVars;

pub(crate) fn main() -> anyhow::Result<()> {
    let reference_string = generate();
    let reference_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("crates")
        .join("ty")
        .join("docs")
        .join("environment.md");

    // Ensure the docs directory exists
    if let Some(parent) = reference_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&reference_path, reference_string)?;
    println!(
        "Generated environment variables reference at: {}",
        reference_path.display()
    );

    Ok(())
}

fn generate() -> String {
    let mut output = String::new();

    output.push_str("# Environment variables\n\n");

    // Partition and sort environment variables into TY_ and external variables.
    let (ty_vars, external_vars): (BTreeSet<_>, BTreeSet<_>) = EnvVars::metadata()
        .iter()
        .partition(|(var, _)| var.starts_with("TY_"));

    output.push_str("ty defines and respects the following environment variables:\n\n");

    for (var, doc) in ty_vars {
        output.push_str(&render(var, doc));
    }

    output.push_str("\n## Externally defined variables\n\n");
    output.push_str("ty also reads the following externally defined environment variables:\n\n");

    for (var, doc) in external_vars {
        output.push_str(&render(var, doc));
    }

    output
}

/// Render an environment variable and its documentation.
fn render(var: &str, doc: &str) -> String {
    format!("### `{var}`\n\n{doc}\n\n")
}
