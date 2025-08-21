//! Generate the environment variables reference from `ty_static::EnvVars`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use anyhow::bail;
use pretty_assertions::StrComparison;

use ty_static::EnvVars;

use crate::generate_all::Mode;

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

pub(crate) fn main(args: &Args) -> anyhow::Result<()> {
    let reference_string = generate();
    let filename = "environment.md";
    let reference_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("crates")
        .join("ty")
        .join("docs")
        .join(filename);

    match args.mode {
        Mode::DryRun => {
            println!("{reference_string}");
        }
        Mode::Check => match fs::read_to_string(&reference_path) {
            Ok(current) => {
                if current == reference_string {
                    println!("Up-to-date: {filename}");
                } else {
                    let comparison = StrComparison::new(&current, &reference_string);
                    bail!(
                        "{filename} changed, please run `cargo dev generate-ty-env-vars-reference`:\n{comparison}"
                    );
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                bail!(
                    "{filename} not found, please run `cargo dev generate-ty-env-vars-reference`"
                );
            }
            Err(err) => {
                bail!(
                    "{filename} changed, please run `cargo dev generate-ty-env-vars-reference`:\n{err}"
                );
            }
        },
        Mode::Write => {
            // Ensure the docs directory exists
            if let Some(parent) = reference_path.parent() {
                fs::create_dir_all(parent)?;
            }

            match fs::read_to_string(&reference_path) {
                Ok(current) => {
                    if current == reference_string {
                        println!("Up-to-date: {filename}");
                    } else {
                        println!("Updating: {filename}");
                        fs::write(&reference_path, reference_string.as_bytes())?;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    println!("Updating: {filename}");
                    fs::write(&reference_path, reference_string.as_bytes())?;
                }
                Err(err) => {
                    bail!(
                        "{filename} changed, please run `cargo dev generate-ty-env-vars-reference`:\n{err}"
                    );
                }
            }
        }
    }

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

    output.push_str("## Externally-defined variables\n\n");
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
