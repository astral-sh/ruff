//! Generate Markdown documentation for applicable rules.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use regex::{Captures, Regex};
use strum::IntoEnumIterator;

use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff_diagnostics::AutofixKind;
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
            output.push_str(&format!("# {} ({})", rule.as_ref(), rule.noqa_code()));
            output.push('\n');
            output.push('\n');

            let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
            if linter.url().is_some() {
                output.push_str(&format!("Derived from the **{}** linter.", linter.name()));
                output.push('\n');
                output.push('\n');
            }

            let autofix = rule.autofixable();
            if matches!(autofix, AutofixKind::Always | AutofixKind::Sometimes) {
                output.push_str(&autofix.to_string());
                output.push('\n');
                output.push('\n');
            }

            if rule.is_preview() {
                output.push_str(
                    r#"This rule is in preview and is not stable. The `--preview` flag is required for use."#,
                );
                output.push('\n');
                output.push('\n');
            }

            process_documentation(explanation.trim(), &mut output);

            let filename = PathBuf::from(ROOT_DIR)
                .join("docs")
                .join("rules")
                .join(rule.as_ref())
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

fn process_documentation(documentation: &str, out: &mut String) {
    let mut in_options = false;
    let mut after = String::new();

    // HACK: This is an ugly regex hack that's necessary because mkdocs uses
    // a non-CommonMark-compliant Markdown parser, which doesn't support code
    // tags in link definitions
    // (see https://github.com/Python-Markdown/markdown/issues/280).
    let documentation = Regex::new(r"\[`([^`]*?)`]($|[^\[])").unwrap().replace_all(
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

                assert!(
                    Options::metadata().get(option).is_some(),
                    "unknown option {option}"
                );

                let anchor = option.replace('.', "-");
                out.push_str(&format!("- [`{option}`][{option}]\n"));
                after.push_str(&format!("[{option}]: ../../settings#{anchor}\n"));

                continue;
            }
        }

        out.push_str(line);
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
See also [`mccabe.max-complexity`] and [`task-tags`].
Something [`else`][other].

## Options

- `task-tags`
- `mccabe.max-complexity`

[other]: http://example.com.",
            &mut output,
        );
        assert_eq!(
            output,
            "
See also [`mccabe.max-complexity`][mccabe.max-complexity] and [`task-tags`][task-tags].
Something [`else`][other].

## Options

- [`task-tags`][task-tags]
- [`mccabe.max-complexity`][mccabe.max-complexity]

[other]: http://example.com.

[task-tags]: ../../settings#task-tags
[mccabe.max-complexity]: ../../settings#mccabe-max-complexity
"
        );
    }
}
