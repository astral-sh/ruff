//! Generates the rules table for ty

use std::borrow::Cow;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use anyhow::{Result, bail};
use itertools::Itertools as _;
use pretty_assertions::StrComparison;

use crate::ROOT_DIR;
use crate::generate_all::{Mode, REGENERATE_ALL_COMMAND};

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
    let registry = ty_python_semantic::default_lint_registry();

    let mut output = String::new();

    let _ = writeln!(
        &mut output,
        "<!-- WARNING: This file is auto-generated (cargo dev generate-all). Edit the lint-declarations in 'crates/ty_python_semantic/src/types/diagnostic.rs' if you want to change anything here. -->\n"
    );
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

        // Reformat headers as bold text
        let mut in_code_fence = false;
        let documentation = lint
            .documentation_lines()
            .map(|line| {
                // Toggle the code fence state if we encounter a boundary
                if line.starts_with("```") {
                    in_code_fence = !in_code_fence;
                }
                if !in_code_fence && line.starts_with('#') {
                    Cow::Owned(format!(
                        "**{line}**\n",
                        line = line.trim_start_matches('#').trim_start()
                    ))
                } else {
                    Cow::Borrowed(line)
                }
            })
            .join("\n");

        let status_text = match lint.status() {
            ty_python_semantic::lint::LintStatus::Stable { since } => {
                format!(
                    r#"Added in <a href="https://github.com/astral-sh/ty/releases/tag/{since}">{since}</a>"#
                )
            }
            ty_python_semantic::lint::LintStatus::Preview { since } => {
                format!(
                    r#"Preview (since <a href="https://github.com/astral-sh/ty/releases/tag/{since}">{since}</a>)"#
                )
            }
            ty_python_semantic::lint::LintStatus::Deprecated { since, .. } => {
                format!(
                    r#"Deprecated (since <a href="https://github.com/astral-sh/ty/releases/tag/{since}">{since}</a>)"#
                )
            }
            ty_python_semantic::lint::LintStatus::Removed { since, .. } => {
                format!(
                    r#"Removed (since <a href="https://github.com/astral-sh/ty/releases/tag/{since}">{since}</a>)"#
                )
            }
        };

        let _ = writeln!(
            &mut output,
            r#"<small>
Default level: <a href="../rules.md#rule-levels" title="This lint has a default level of '{level}'."><code>{level}</code></a> ·
{status_text} ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20{encoded_name}" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/{file}#L{line}" target="_blank">View source</a>
</small>


{documentation}
"#,
            level = lint.default_level(),
            encoded_name = url::form_urlencoded::byte_serialize(lint.name().as_str().as_bytes())
                .collect::<String>(),
            file = url::form_urlencoded::byte_serialize(lint.file().replace('\\', "/").as_bytes())
                .collect::<String>(),
            line = lint.line(),
        );
    }

    output
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::generate_all::Mode;

    use super::{Args, main};

    #[test]
    fn ty_rules_up_to_date() -> Result<()> {
        main(&Args { mode: Mode::Check })
    }
}
