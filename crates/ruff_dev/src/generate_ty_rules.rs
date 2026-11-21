//! Generates the rules table for ty

use std::borrow::Cow;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Result, bail};
use itertools::Itertools as _;
use regex::{Captures, Regex};
use rustc_hash::FxHashMap;

use ruff_db::diagnostic::LintName;
use ty_python_semantic::default_lint_registry;
use ty_python_semantic::lint::{LintId, LintStatus};

use crate::ROOT_DIR;
use crate::generate_all::{Mode, REGENERATE_ALL_COMMAND, generated_file_diff};

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
                let comparison = generated_file_diff(&current, &markdown);
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
    let registry = default_lint_registry();

    let mut output = String::new();

    let _ = writeln!(
        &mut output,
        "<!-- WARNING: This file is auto-generated (cargo dev generate-all). Edit the lint-declarations in 'crates/ty_python_semantic/src/types/diagnostic.rs' if you want to change anything here. -->\n"
    );
    let _ = writeln!(&mut output, "# Rules\n");

    let mut aliases: FxHashMap<LintId, Vec<LintName>> = FxHashMap::default();
    for (name, lint) in registry.aliases().sorted_by_key(|(name, _)| *name) {
        aliases.entry(lint).or_default().push(name);
    }

    for &lint in registry.lints().iter().sorted_by_key(|lint| lint.name()) {
        write_rule(&mut output, lint, &aliases);
    }

    let removed = registry.removed().sorted_by_key(|lint| lint.name());
    if !removed.as_slice().is_empty() {
        output.push_str("## Removed rules\n\n");
        for lint in removed {
            write_rule(&mut output, lint, &aliases);
        }
    }

    output
}

fn write_rule(output: &mut String, lint: LintId, aliases: &FxHashMap<LintId, Vec<LintName>>) {
    // Keep links to the old rule heading working after a rename.
    let names = aliases.get(&lint).map_or_default(Vec::as_slice);
    if !names.is_empty() {
        for name in names {
            let _ = writeln!(output, r#"<a id="{name}"></a>"#);
        }
        output.push('\n');
    }

    // Reformat headers as bold text
    let mut in_code_fence = false;
    let documentation = lint
        .documentation_lines()
        .map(|line| {
            // Toggle the code fence state if we encounter a boundary
            if line.starts_with("```") {
                in_code_fence = !in_code_fence;
            }
            if !in_code_fence {
                if line.starts_with('#') {
                    Cow::Owned(format!(
                        "**{line}**\n",
                        line = line.trim_start_matches('#').trim_start()
                    ))
                } else {
                    link_rule_names(line)
                }
            } else {
                Cow::Borrowed(line)
            }
        })
        .join("\n");

    let status_text = match lint.status() {
        LintStatus::Preview { since } => {
            format!(
                r#"Preview (since <a href="https://github.com/astral-sh/ty/releases/tag/{since}">{since}</a>)"#
            )
        }
        LintStatus::Stable { since } => {
            format!(
                r#"Added in <a href="https://github.com/astral-sh/ty/releases/tag/{since}">{since}</a>"#
            )
        }
        LintStatus::Deprecated { since, .. } => {
            format!(
                r#"Deprecated (since <a href="https://github.com/astral-sh/ty/releases/tag/{since}">{since}</a>)"#
            )
        }
        LintStatus::Removed { since, reason } => {
            let name = lint.name();
            let reason = link_rule_names(reason);
            let _ = writeln!(output, "### `{name}`\n");
            let _ = writeln!(
                output,
                "Removed in [{since}](https://github.com/astral-sh/ty/releases/tag/{since}).\n\n**Why was this rule removed?**\n\n{reason}\n"
            );
            let _ = writeln!(output, "{documentation}\n");
            return;
        }
    };

    let _ = writeln!(output, "## `{rule_name}`\n", rule_name = lint.name());

    let quoted_name = |name: LintName| format!("\"{name}\"");
    let search_terms = if names.is_empty() {
        quoted_name(lint.name())
    } else {
        let terms = std::iter::once(lint.name())
            .chain(names.iter().copied())
            .map(quoted_name)
            .join(" OR ");
        format!("({terms})")
    };

    let _ = writeln!(
        output,
        r#"<small>
Default level: <a href="../../rules#rule-levels" title="This lint has a default level of '{level}'."><code>{level}</code></a> ·
{status_text} ·
<a href="https://github.com/astral-sh/ty/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20{encoded_terms}" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/{file}#L{line}" target="_blank">View source</a>
</small>


{documentation}
"#,
        level = lint.default_level(),
        encoded_terms =
            url::form_urlencoded::byte_serialize(search_terms.as_bytes()).collect::<String>(),
        file = url::form_urlencoded::byte_serialize(lint.file().replace('\\', "/").as_bytes())
            .collect::<String>(),
        line = lint.line(),
    );
}

// Replace `rule-name` with [`rule-name`](url) for any known rule names,
// turning them into inline links automatically.
fn link_rule_names(text: &str) -> Cow<'_, str> {
    static RULE_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").unwrap());
    RULE_LINK.replace_all(text, |caps: &Captures| {
        let name = &caps[1];
        if default_lint_registry().get(name).is_ok() {
            format!("[`{name}`](#{name})")
        } else {
            caps[0].to_string()
        }
    })
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
