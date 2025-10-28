//! Generate Markdown documentation for applicable rules.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use itertools::Itertools;
use regex::{Captures, Regex};
use ruff_linter::codes::RuleGroup;
use strum::IntoEnumIterator;

use ruff_linter::FixAvailability;
use ruff_linter::registry::{Linter, Rule, RuleNamespace};
use ruff_options_metadata::{OptionEntry, OptionsMetadata};
use ruff_workspace::options::Options;

use crate::ROOT_DIR;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Write the generated docs to stdout (rather than to the filesystem).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    for rule in Rule::iter() {
        if let Some(explanation) = rule.explanation() {
            let mut output = String::new();

            let _ = writeln!(&mut output, "# {} ({})", rule.name(), rule.noqa_code());

            let status_text = match rule.group() {
                RuleGroup::Stable { since } => {
                    format!(
                        r#"Added in <a href="https://github.com/astral-sh/ruff/releases/tag/{since}">{since}</a>"#
                    )
                }
                RuleGroup::Preview { since } => {
                    format!(
                        r#"Preview (since <a href="https://github.com/astral-sh/ruff/releases/tag/{since}">{since}</a>)"#
                    )
                }
                RuleGroup::Deprecated { since } => {
                    format!(
                        r#"Deprecated (since <a href="https://github.com/astral-sh/ruff/releases/tag/{since}">{since}</a>)"#
                    )
                }
                RuleGroup::Removed { since } => {
                    format!(
                        r#"Removed (since <a href="https://github.com/astral-sh/ruff/releases/tag/{since}">{since}</a>)"#
                    )
                }
            };

            let _ = writeln!(
                &mut output,
                r#"<small>
{status_text} ·
<a href="https://github.com/astral-sh/ruff/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20(%27{encoded_name}%27%20OR%20{rule_code})" target="_blank">Related issues</a> ·
<a href="https://github.com/astral-sh/ruff/blob/main/{file}#L{line}" target="_blank">View source</a>
</small>

"#,
                encoded_name =
                    url::form_urlencoded::byte_serialize(rule.name().as_str().as_bytes())
                        .collect::<String>(),
                rule_code = rule.noqa_code(),
                file =
                    url::form_urlencoded::byte_serialize(rule.file().replace('\\', "/").as_bytes())
                        .collect::<String>(),
                line = rule.line(),
            );
            let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
            if linter.url().is_some() {
                let common_prefix: String = match linter.common_prefix() {
                    "" => linter
                        .upstream_categories()
                        .unwrap()
                        .iter()
                        .map(|c| c.prefix)
                        .join("-"),
                    prefix => prefix.to_string(),
                };
                let anchor = format!(
                    "{}-{}",
                    linter.name().to_lowercase(),
                    common_prefix.to_lowercase()
                );

                let _ = write!(
                    output,
                    "Derived from the **[{}](../rules.md#{})** linter.",
                    linter.name(),
                    anchor,
                );
                output.push('\n');
                output.push('\n');
            }

            if rule.is_deprecated() {
                output.push_str(
                    r"**Warning: This rule is deprecated and will be removed in a future release.**",
                );
                output.push('\n');
                output.push('\n');
            }

            if rule.is_removed() {
                output.push_str(
                    r"**Warning: This rule has been removed and its documentation is only available for historical reasons.**",
                );
                output.push('\n');
                output.push('\n');
            }

            let fix_availability = rule.fixable();
            if matches!(
                fix_availability,
                FixAvailability::Always | FixAvailability::Sometimes
            ) {
                output.push_str(&fix_availability.to_string());
                output.push('\n');
                output.push('\n');
            }

            if rule.is_preview() {
                output.push_str(
                    r"This rule is unstable and in [preview](../preview.md). The `--preview` flag is required for use.",
                );
                output.push('\n');
                output.push('\n');
            }

            process_documentation(
                explanation.trim(),
                &mut output,
                &rule.noqa_code().to_string(),
            );

            let filename = PathBuf::from(ROOT_DIR)
                .join("docs")
                .join("rules")
                .join(&*rule.name())
                .with_extension("md");

            if args.dry_run {
                println!("{output}");
            } else {
                fs::create_dir_all("docs/rules")?;
                fs::write(filename, output)?;
            }
        }
    }
    Ok(())
}

fn process_documentation(documentation: &str, out: &mut String, rule_name: &str) {
    let mut in_options = false;
    let mut after = String::new();
    let mut referenced_options = HashSet::new();

    // HACK: This is an ugly regex hack that's necessary because mkdocs uses
    // a non-CommonMark-compliant Markdown parser, which doesn't support code
    // tags in link definitions
    // (see https://github.com/Python-Markdown/markdown/issues/280).
    let documentation = Regex::new(r"\[`([^`]*?)`]($|[^\[(])").unwrap().replace_all(
        documentation,
        |caps: &Captures| {
            format!(
                "[`{option}`][{option}]{sep}",
                option = &caps[1],
                sep = &caps[2]
            )
        },
    );

    for line in documentation.split_inclusive('\n') {
        if line.starts_with("## ") {
            in_options = line == "## Options\n";
        } else if in_options {
            if let Some(rest) = line.strip_prefix("- `") {
                let option = rest.trim_end().trim_end_matches('`');

                match Options::metadata().find(option) {
                    Some(OptionEntry::Field(field)) => {
                        if field.deprecated.is_some() {
                            eprintln!("Rule {rule_name} references deprecated option {option}.");
                        }
                    }
                    Some(_) => {}
                    None => {
                        panic!("Unknown option {option} referenced by rule {rule_name}");
                    }
                }

                let anchor = option.replace('.', "_");
                let _ = writeln!(out, "- [`{option}`][{option}]");
                let _ = writeln!(&mut after, "[{option}]: ../settings.md#{anchor}");
                referenced_options.insert(option);

                continue;
            }
        }

        out.push_str(line);
    }

    let re = Regex::new(r"\[`([^`]*?)`]\[(.*?)]").unwrap();
    for (_, [option, _]) in re.captures_iter(&documentation).map(|c| c.extract()) {
        if let Some(OptionEntry::Field(field)) = Options::metadata().find(option) {
            if referenced_options.insert(option) {
                let anchor = option.replace('.', "_");
                let _ = writeln!(&mut after, "[{option}]: ../settings.md#{anchor}");
            }
            if field.deprecated.is_some() {
                eprintln!("Rule {rule_name} references deprecated option {option}.");
            }
        }
    }

    if !after.is_empty() {
        out.push('\n');
        out.push('\n');
        out.push_str(&after);
    }
}

#[cfg(test)]
mod tests {
    use super::process_documentation;

    #[test]
    fn test_process_documentation() {
        let mut output = String::new();
        process_documentation(
            "
See also [`lint.mccabe.max-complexity`] and [`lint.task-tags`].
Something [`else`][other]. Some [link](https://example.com).

## Options

- `lint.task-tags`
- `lint.mccabe.max-complexity`

[other]: http://example.com.",
            &mut output,
            "example",
        );
        assert_eq!(
            output,
            "
See also [`lint.mccabe.max-complexity`][lint.mccabe.max-complexity] and [`lint.task-tags`][lint.task-tags].
Something [`else`][other]. Some [link](https://example.com).

## Options

- [`lint.task-tags`][lint.task-tags]
- [`lint.mccabe.max-complexity`][lint.mccabe.max-complexity]

[other]: http://example.com.

[lint.task-tags]: ../settings.md#lint_task-tags
[lint.mccabe.max-complexity]: ../settings.md#lint_mccabe_max-complexity
"
        );
    }
}
