//! Generate a Markdown-compatible listing of configuration options.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use itertools::Itertools;
use ruff::settings::options::Options;
use ruff::settings::options_base::{ConfigurationOptions, OptionEntry, OptionField};

const BEGIN_PRAGMA: &str = "<!-- Begin auto-generated options sections. -->";
const END_PRAGMA: &str = "<!-- End auto-generated options sections. -->";

#[derive(Args)]
pub struct Cli {
    /// Write the generated table to stdout (rather than to `README.md`).
    #[arg(long)]
    dry_run: bool,
}

fn emit_field(output: &mut String, field: &OptionField, group_name: Option<&str>) {
    output.push_str(&format!("#### [`{0}`](#{0})\n", field.name));
    output.push('\n');
    output.push_str(field.doc);
    output.push_str("\n\n");
    output.push_str(&format!("**Default value**: `{}`\n", field.default));
    output.push('\n');
    output.push_str(&format!("**Type**: `{}`\n", field.value_type));
    output.push('\n');
    output.push_str(&format!(
        "**Example usage**:\n\n```toml\n[tool.ruff{}]\n{}\n```\n",
        if group_name.is_some() {
            format!(".{}", group_name.unwrap())
        } else {
            String::new()
        },
        field.example
    ));
    output.push('\n');
}

pub fn main(cli: &Cli) -> Result<()> {
    let mut output = String::new();

    // Generate all the top-level fields.
    for field in Options::get_available_options()
        .into_iter()
        .filter_map(|entry| {
            if let OptionEntry::Field(field) = entry {
                Some(field)
            } else {
                None
            }
        })
        .sorted_by_key(|field| field.name)
    {
        emit_field(&mut output, &field, None);
        output.push_str("---\n\n");
    }

    // Generate all the sub-groups.
    for group in Options::get_available_options()
        .into_iter()
        .filter_map(|entry| {
            if let OptionEntry::Group(group) = entry {
                Some(group)
            } else {
                None
            }
        })
        .sorted_by_key(|group| group.name)
    {
        output.push_str(&format!("### `{}`\n", group.name));
        output.push('\n');
        for field in group
            .fields
            .iter()
            .filter_map(|entry| {
                if let OptionEntry::Field(field) = entry {
                    Some(field)
                } else {
                    None
                }
            })
            .sorted_by_key(|field| field.name)
        {
            emit_field(&mut output, field, Some(group.name));
            output.push_str("---\n\n");
        }
    }

    if cli.dry_run {
        print!("{output}");
    } else {
        // Read the existing file.
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to find root directory")
            .join("README.md");
        let existing = fs::read_to_string(&file)?;

        // Extract the prefix.
        let index = existing
            .find(BEGIN_PRAGMA)
            .expect("Unable to find begin pragma");
        let prefix = &existing[..index + BEGIN_PRAGMA.len()];

        // Extract the suffix.
        let index = existing
            .find(END_PRAGMA)
            .expect("Unable to find end pragma");
        let suffix = &existing[index..];

        // Write the prefix, new contents, and suffix.
        let mut f = OpenOptions::new().write(true).truncate(true).open(&file)?;
        write!(f, "{prefix}\n\n")?;
        write!(f, "{output}")?;
        write!(f, "{suffix}")?;
    }

    Ok(())
}
