//! Generate a Markdown-compatible listing of configuration options.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::Result;
use itertools::Itertools;
use ruff::settings::options::Options;
use ruff::settings::options_base::{ConfigurationOptions, OptionEntry, OptionField};

use crate::utils::replace_readme_section;

const BEGIN_PRAGMA: &str = "<!-- Begin auto-generated options sections. -->\n";
const END_PRAGMA: &str = "<!-- End auto-generated options sections. -->";

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated table to stdout (rather than to `README.md`).
    #[arg(long)]
    pub(crate) dry_run: bool,
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

pub fn main(args: &Args) -> Result<()> {
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

    if args.dry_run {
        print!("{output}");
    } else {
        replace_readme_section(&output, BEGIN_PRAGMA, END_PRAGMA)?;
    }

    Ok(())
}
