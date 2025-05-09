//! Generates the rules table for ty

use std::borrow::Cow;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};
use itertools::Itertools as _;
use pretty_assertions::StrComparison;

use crate::generate_all::{Mode, REGENERATE_ALL_COMMAND};
use crate::ROOT_DIR;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Write the generated table to stdout (rather than to `ty.schema.json`).
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let markdown = generate_markdown();
    let filename = "crates/ty/docs/rules.md";
    let schema_path = PathBuf::from(ROOT_DIR).join(filename);

    match args.mode {
        Mode::DryRun => {
            println!("{markdown}");
        }
        Mode::Check => {
            let current = fs::read_to_string(schema_path)?;
            if current == markdown {
                println!("Up-to-date: {filename}");
            } else {
                let comparison = StrComparison::new(&current, &markdown);
                bail!("{filename} changed, please run `{REGENERATE_ALL_COMMAND}`:\n{comparison}");
            }
        }
        Mode::Write => {
            let current = fs::read_to_string(&schema_path)?;
            if current == markdown {
                println!("Up-to-date: {filename}");
            } else {
                println!("Updating: {filename}");
                fs::write(schema_path, markdown.as_bytes())?;
            }
        }
    }

    Ok(())
}

fn generate_markdown() -> String {
    let registry = &*ty_project::DEFAULT_LINT_REGISTRY;

    let mut output = String::new();

    let _ = writeln!(&mut output, "# Rules\n");

    let mut lints: Vec<_> = registry.lints().iter().collect();
    lints.sort_by(|a, b| {
        a.default_level()
            .cmp(&b.default_level())
            .reverse()
            .then_with(|| a.name().cmp(&b.name()))
    });

    for lint in lints {
        let _ = writeln!(&mut output, "## `{rule_name}`\n", rule_name = lint.name());

        // Increase the header-level by one
        let documentation = lint
            .documentation_lines()
            .map(|line| {
                if line.starts_with('#') {
                    Cow::Owned(format!("#{line}"))
                } else {
                    Cow::Borrowed(line)
                }
            })
            .join("\n");

        let _ = writeln!(
            &mut output,
            r#"**Default level**: {level}

<details>
<summary>{summary}</summary>

{documentation}

### Links
* [Related issues](https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20{encoded_name})
* [View source](https://github.com/astral-sh/ruff/blob/main/{file}#L{line})
</details>
"#,
            level = lint.default_level(),
            // GitHub doesn't support markdown in `summary` headers
            summary = replace_inline_code(lint.summary()),
            encoded_name = url::form_urlencoded::byte_serialize(lint.name().as_str().as_bytes())
                .collect::<String>(),
            file = url::form_urlencoded::byte_serialize(lint.file().replace('\\', "/").as_bytes())
                .collect::<String>(),
            line = lint.line(),
        );
    }

    output
}

/// Replaces inline code blocks (`code`) with `<code>code</code>`
fn replace_inline_code(input: &str) -> String {
    let mut output = String::new();
    let mut parts = input.split('`');

    while let Some(before) = parts.next() {
        if let Some(between) = parts.next() {
            output.push_str(before);
            output.push_str("<code>");
            output.push_str(between);
            output.push_str("</code>");
        } else {
            output.push_str(before);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::generate_all::Mode;

    use super::{main, Args};

    #[test]
    fn ty_rules_up_to_date() -> Result<()> {
        main(&Args { mode: Mode::Check })
    }
}
