//! Generate Markdown documentation for applicable rules.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;

use anyhow::Result;
use regex::{Captures, Regex};
use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff::settings::options::Options;
use ruff::settings::options_base::ConfigurationOptions;
use ruff::AutofixAvailability;
use strum::IntoEnumIterator;

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated docs to stdout (rather than to the filesystem).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub fn main(args: &Args) -> Result<()> {
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

            if let Some(autofix) = rule.autofixable() {
                output.push_str(match autofix.available {
                    AutofixAvailability::Sometimes => "Autofix is sometimes available.",
                    AutofixAvailability::Always => "Autofix is always available.",
                });
                output.push('\n');
                output.push('\n');
            }

            process_documentation(explanation.trim(), &mut output);

            if args.dry_run {
                println!("{output}");
            } else {
                fs::create_dir_all("docs/rules")?;
                fs::write(format!("docs/rules/{}.md", rule.as_ref()), output)?;
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
    let documentation = Regex::new(r"\[`(.*?)`\]($|[^\[])").unwrap().replace_all(
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
                    Options::get(Some(option)).is_some(),
                    "unknown option {option}"
                );

                let anchor = option.rsplit('.').next().unwrap();
                out.push_str(&format!("- [`{option}`][{option}]\n"));
                after.push_str(&format!("[{option}]: ../../settings#{anchor}"));

                continue;
            }
        }

        out.push_str(line);
    }
    if !after.is_empty() {
        out.push_str("\n\n");
        out.push_str(&after);
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::process_documentation;

    #[test]
    fn test_process_documentation() {
        let mut out = String::new();
        process_documentation(
            "
See also [`mccabe.max-complexity`].
Something [`else`][other].

## Options

- `mccabe.max-complexity`

[other]: http://example.com.",
            &mut out,
        );
        assert_eq!(
            out,
            "
See also [`mccabe.max-complexity`][mccabe.max-complexity].
Something [`else`][other].

## Options

- [`mccabe.max-complexity`][mccabe.max-complexity]

[other]: http://example.com.

[mccabe.max-complexity]: ../../settings#max-complexity\n"
        );
    }
}
