use std::io::{self, BufWriter, Write};

use anyhow::Result;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use strum::IntoEnumIterator;

use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff_diagnostics::AutofixKind;

use crate::args::HelpFormat;

#[derive(Serialize)]
struct Explanation<'a> {
    name: &'a str,
    code: String,
    linter: &'a str,
    summary: &'a str,
    message_formats: &'a [&'a str],
    autofix: String,
    explanation: Option<&'a str>,
    preview: bool,
}

impl<'a> Explanation<'a> {
    fn from_rule(rule: &'a Rule) -> Self {
        let code = rule.noqa_code().to_string();
        let (linter, _) = Linter::parse_code(&code).unwrap();
        let autofix = rule.autofixable().to_string();
        Self {
            name: rule.as_ref(),
            code,
            linter: linter.name(),
            summary: rule.message_formats()[0],
            message_formats: rule.message_formats(),
            autofix,
            explanation: rule.explanation(),
            preview: rule.is_preview(),
        }
    }
}

fn format_rule_text(rule: Rule) -> String {
    let mut output = String::new();
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

    if rule.is_preview() {
        output.push_str(
            r#"This rule is in preview and is not stable. The `--preview` flag is required for use."#,
        );
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
    output
}

/// Explain a `Rule` to the user.
pub(crate) fn rule(rule: Rule, format: HelpFormat) -> Result<()> {
    let mut stdout = BufWriter::new(io::stdout().lock());
    match format {
        HelpFormat::Text => {
            writeln!(stdout, "{}", format_rule_text(rule))?;
        }
        HelpFormat::Json => {
            serde_json::to_writer_pretty(stdout, &Explanation::from_rule(&rule))?;
        }
    };
    Ok(())
}

/// Explain all rules to the user.
pub(crate) fn rules(format: HelpFormat) -> Result<()> {
    let mut stdout = BufWriter::new(io::stdout().lock());
    match format {
        HelpFormat::Text => {
            for rule in Rule::iter() {
                writeln!(stdout, "{}", format_rule_text(rule))?;
                writeln!(stdout)?;
            }
        }
        HelpFormat::Json => {
            let mut serializer = serde_json::Serializer::pretty(stdout);
            let mut seq = serializer.serialize_seq(None)?;
            for rule in Rule::iter() {
                seq.serialize_element(&Explanation::from_rule(&rule))?;
            }
            seq.end()?;
        }
    }
    Ok(())
}
