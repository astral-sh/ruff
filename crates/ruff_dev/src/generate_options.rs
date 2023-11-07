//! Generate a Markdown-compatible listing of configuration options for `pyproject.toml`.
//!
//! Used for <https://docs.astral.sh/ruff/settings/>.
use std::fmt::Write;

use ruff_python_trivia::textwrap;
use ruff_workspace::options::Options;
use ruff_workspace::options_base::{OptionField, OptionSet, OptionsMetadata, Visit};

pub(crate) fn generate() -> String {
    let mut output = String::new();
    generate_set(&mut output, &Set::Toplevel(Options::metadata()));

    output
}

fn generate_set(output: &mut String, set: &Set) {
    if set.level() < 2 {
        writeln!(output, "### {title}\n", title = set.title()).unwrap();
    } else {
        writeln!(output, "#### {title}\n", title = set.title()).unwrap();
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

    // Generate the fields.
    for (name, field) in &fields {
        emit_field(output, name, field, set);
        output.push_str("---\n\n");
    }

    // Generate all the sub-sets.
    for (set_name, sub_set) in &sets {
        generate_set(output, &Set::Named(set_name, *sub_set, set.level() + 1));
    }
}

enum Set<'a> {
    Toplevel(OptionSet),
    Named(&'a str, OptionSet, u32),
}

impl<'a> Set<'a> {
    fn name(&self) -> Option<&'a str> {
        match self {
            Set::Toplevel(_) => None,
            Set::Named(name, _, _) => Some(name),
        }
    }

    fn title(&self) -> &'a str {
        match self {
            Set::Toplevel(_) => "Top-level",
            Set::Named(name, _, _) => name,
        }
    }

    fn metadata(&self) -> &OptionSet {
        match self {
            Set::Toplevel(set) => set,
            Set::Named(_, set, _) => set,
        }
    }

    fn level(&self) -> u32 {
        match self {
            Set::Toplevel(_) => 0,
            Set::Named(_, _, level) => *level,
        }
    }
}

fn emit_field(output: &mut String, name: &str, field: &OptionField, parent_set: &Set) {
    let header_level = if parent_set.level() < 2 {
        "####"
    } else {
        "#####"
    };

    // if there's a set name, we need to add it to the anchor
    if let Some(set_name) = parent_set.name() {
        // the anchor used to just be the name, but now it's the group name
        // for backwards compatibility, we need to keep the old anchor
        output.push_str(&format!("<span id=\"{name}\"></span>\n"));

        output.push_str(&format!(
            "{header_level} [`{name}`](#{set_name}-{name}) {{: #{set_name}-{name} }}\n"
        ));
    } else {
        output.push_str(&format!("{header_level} [`{name}`](#{name})\n"));
    }
    output.push('\n');

    if let Some(deprecated) = &field.deprecated {
        output.push_str("!!! warning \"Deprecated\"\n");
        output.push_str("    This option has been deprecated");

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
    output.push_str(&format!("**Default value**: `{}`\n", field.default));
    output.push('\n');
    output.push_str(&format!("**Type**: `{}`\n", field.value_type));
    output.push('\n');
    output.push_str("**Example usage**:\n\n");
    output.push_str(&format_tab(
        "pyproject.toml",
        &format_header(parent_set, ConfigurationFile::PyprojectToml),
        field.example,
    ));
    output.push_str(&format_tab(
        "ruff.toml",
        &format_header(parent_set, ConfigurationFile::RuffToml),
        field.example,
    ));
    output.push('\n');
}

fn format_tab(tab_name: &str, header: &str, content: &str) -> String {
    format!(
        "=== \"{}\"\n\n    ```toml\n    {}\n{}\n    ```\n",
        tab_name,
        header,
        textwrap::indent(content, "    ")
    )
}

fn format_header(parent_set: &Set, configuration: ConfigurationFile) -> String {
    let fmt = if let Some(set_name) = parent_set.name() {
        if set_name == "format" {
            String::from(".format")
        } else {
            format!(".lint.{set_name}")
        }
    } else {
        String::new()
    };
    match configuration {
        ConfigurationFile::PyprojectToml => format!("[tool.ruff{fmt}]"),
        ConfigurationFile::RuffToml => {
            if fmt.is_empty() {
                String::new()
            } else {
                format!("[{}]", fmt.strip_prefix('.').unwrap())
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum ConfigurationFile {
    PyprojectToml,
    RuffToml,
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
