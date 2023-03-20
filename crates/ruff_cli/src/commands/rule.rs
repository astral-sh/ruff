use std::io::{self, BufWriter, Write};

use anyhow::Result;
use serde::Serialize;

use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff_diagnostics::AutofixKind;

use crate::args::HelpFormat;

#[derive(Serialize)]
struct Explanation<'a> {
    code: &'a str,
    linter: &'a str,
    summary: &'a str,
}

/// Explain a `Rule` to the user.
pub fn rule(rule: Rule, format: HelpFormat) -> Result<()> {
    let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
    let mut stdout = BufWriter::new(io::stdout().lock());
    let mut output = String::new();

    match format {
        HelpFormat::Text => {
            output.push_str(&format!("# {} ({})", rule.as_ref(), rule.noqa_code()));
            output.push('\n');
            output.push('\n');

            let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
            output.push_str(&format!("Derived from the **{}** linter.", linter.name()));
            output.push('\n');
            output.push('\n');

            let autofix = rule.autofixable();
            if matches!(autofix, AutofixKind::Always | AutofixKind::Sometimes) {
                output.push_str(&autofix.to_string());
                output.push('\n');
                output.push('\n');
            }

            if let Some(explanation) = rule.explanation() {
                output.push_str(explanation.trim());
            } else {
                output.push_str("Message formats:");
                for format in rule.message_formats() {
                    output.push('\n');
                    output.push_str(&format!("* {format}"));
                }
            }
        }
        HelpFormat::Json => {
            output.push_str(&serde_json::to_string_pretty(&Explanation {
                code: &rule.noqa_code().to_string(),
                linter: linter.name(),
                summary: rule.message_formats()[0],
            })?);
        }
    };

    writeln!(stdout, "{output}")?;

    Ok(())
}
