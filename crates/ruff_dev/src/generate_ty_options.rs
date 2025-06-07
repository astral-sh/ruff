//! Generate a Markdown-compatible listing of configuration options for `pyproject.toml`.

use anyhow::bail;
use itertools::Itertools;
use pretty_assertions::StrComparison;
use std::{fmt::Write, path::PathBuf};

use ruff_options_metadata::{OptionField, OptionSet, OptionsMetadata, Visit};
use ty_project::metadata::Options;

use crate::{
    ROOT_DIR,
    generate_all::{Mode, REGENERATE_ALL_COMMAND},
};

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Write the generated table to stdout (rather than to `crates/ty/docs/configuration.md`).
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

pub(crate) fn main(args: &Args) -> anyhow::Result<()> {
    let mut output = String::new();
    let file_name = "crates/ty/docs/configuration.md";
    let markdown_path = PathBuf::from(ROOT_DIR).join(file_name);

    output.push_str(
        "<!-- WARNING: This file is auto-generated (cargo dev generate-all). Update the doc comments on the 'Options' struct in 'crates/ty_project/src/metadata/options.rs' if you want to change anything here. -->\n\n",
    );

    generate_set(
        &mut output,
        Set::Toplevel(Options::metadata()),
        &mut Vec::new(),
    );

    match args.mode {
        Mode::DryRun => {
            println!("{output}");
        }
        Mode::Check => {
            let current = std::fs::read_to_string(&markdown_path)?;
            if output == current {
                println!("Up-to-date: {file_name}",);
            } else {
                let comparison = StrComparison::new(&current, &output);
                bail!("{file_name} changed, please run `{REGENERATE_ALL_COMMAND}`:\n{comparison}",);
            }
        }
        Mode::Write => {
            let current = std::fs::read_to_string(&markdown_path)?;
            if current == output {
                println!("Up-to-date: {file_name}",);
            } else {
                println!("Updating: {file_name}",);
                std::fs::write(markdown_path, output.as_bytes())?;
            }
        }
    }

    Ok(())
}

fn generate_set(output: &mut String, set: Set, parents: &mut Vec<Set>) {
    match &set {
        Set::Toplevel(_) => {
            output.push_str("# Configuration\n");
        }
        Set::Named { name, .. } => {
            let title = parents
                .iter()
                .filter_map(|set| set.name())
                .chain(std::iter::once(name.as_str()))
                .join(".");
            writeln!(output, "## `{title}`\n",).unwrap();
        }
    }

    if let Some(documentation) = set.metadata().documentation() {
        output.push_str(documentation);
        output.push('\n');
        output.push('\n');
    }

    let mut visitor = CollectOptionsVisitor::default();
    set.metadata().record(&mut visitor);

    let (mut fields, mut sets) = (visitor.fields, visitor.groups);

    fields.sort_unstable_by(|(name, _), (name2, _)| name.cmp(name2));
    sets.sort_unstable_by(|(name, _), (name2, _)| name.cmp(name2));

    parents.push(set);

    // Generate the fields.
    for (name, field) in &fields {
        emit_field(output, name, field, parents.as_slice());
        output.push_str("---\n\n");
    }

    // Generate all the sub-sets.
    for (set_name, sub_set) in &sets {
        generate_set(
            output,
            Set::Named {
                name: set_name.to_string(),
                set: *sub_set,
            },
            parents,
        );
    }

    parents.pop();
}

enum Set {
    Toplevel(OptionSet),
    Named { name: String, set: OptionSet },
}

impl Set {
    fn name(&self) -> Option<&str> {
        match self {
            Set::Toplevel(_) => None,
            Set::Named { name, .. } => Some(name),
        }
    }

    fn metadata(&self) -> &OptionSet {
        match self {
            Set::Toplevel(set) => set,
            Set::Named { set, .. } => set,
        }
    }
}

fn emit_field(output: &mut String, name: &str, field: &OptionField, parents: &[Set]) {
    let header_level = if parents.is_empty() { "###" } else { "####" };

    let _ = writeln!(output, "{header_level} `{name}`");

    output.push('\n');

    if let Some(deprecated) = &field.deprecated {
        output.push_str("> [!WARN] \"Deprecated\"\n");
        output.push_str("> This option has been deprecated");

        if let Some(since) = deprecated.since {
            write!(output, " in {since}").unwrap();
        }

        output.push('.');

        if let Some(message) = deprecated.message {
            writeln!(output, " {message}").unwrap();
        }

        output.push('\n');
    }

    output.push_str(field.doc);
    output.push_str("\n\n");
    let _ = writeln!(output, "**Default value**: `{}`", field.default);
    output.push('\n');
    let _ = writeln!(output, "**Type**: `{}`", field.value_type);
    output.push('\n');
    output.push_str("**Example usage** (`pyproject.toml`):\n\n");
    output.push_str(&format_example(
        &format_header(
            field.scope,
            field.example,
            parents,
            ConfigurationFile::PyprojectToml,
        ),
        field.example,
    ));
    output.push('\n');
}

fn format_example(header: &str, content: &str) -> String {
    if header.is_empty() {
        format!("```toml\n{content}\n```\n",)
    } else {
        format!("```toml\n{header}\n{content}\n```\n",)
    }
}

/// Format the TOML header for the example usage for a given option.
///
/// For example: `[tool.ruff.format]` or `[tool.ruff.lint.isort]`.
fn format_header(
    scope: Option<&str>,
    example: &str,
    parents: &[Set],
    configuration: ConfigurationFile,
) -> String {
    let tool_parent = match configuration {
        ConfigurationFile::PyprojectToml => Some("tool.ty"),
        ConfigurationFile::TyToml => None,
    };

    let header = tool_parent
        .into_iter()
        .chain(parents.iter().filter_map(|parent| parent.name()))
        .chain(scope)
        .join(".");

    // Ex) `[[tool.ty.xx]]`
    if example.starts_with(&format!("[[{header}")) {
        return String::new();
    }
    // Ex) `[tool.ty.rules]`
    if example.starts_with(&format!("[{header}")) {
        return String::new();
    }

    if header.is_empty() {
        String::new()
    } else {
        format!("[{header}]")
    }
}

#[derive(Default)]
struct CollectOptionsVisitor {
    groups: Vec<(String, OptionSet)>,
    fields: Vec<(String, OptionField)>,
}

impl Visit for CollectOptionsVisitor {
    fn record_set(&mut self, name: &str, group: OptionSet) {
        self.groups.push((name.to_owned(), group));
    }

    fn record_field(&mut self, name: &str, field: OptionField) {
        self.fields.push((name.to_owned(), field));
    }
}

#[derive(Debug, Copy, Clone)]
enum ConfigurationFile {
    PyprojectToml,
    #[expect(dead_code)]
    TyToml,
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::generate_all::Mode;

    use super::{Args, main};

    #[test]
    fn ty_configuration_markdown_up_to_date() -> Result<()> {
        main(&Args { mode: Mode::Check })?;
        Ok(())
    }
}
