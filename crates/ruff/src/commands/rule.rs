use std::fmt::Write as _;
use std::io::{self, BufWriter, Write};

use anyhow::Result;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use strum::IntoEnumIterator;

use ruff_linter::FixAvailability;
use ruff_linter::codes::RuleGroup;
use ruff_linter::registry::{Linter, Rule, RuleNamespace};

use crate::args::HelpFormat;

#[derive(Serialize)]
struct Explanation<'a> {
    name: &'a str,
    code: String,
    linter: &'a str,
    summary: &'a str,
    message_formats: &'a [&'a str],
    fix: String,
    fix_availability: FixAvailability,
    #[expect(clippy::struct_field_names)]
    explanation: Option<&'a str>,
    preview: bool,
    status: RuleGroup,
    source_location: SourceLocation,
}

impl<'a> Explanation<'a> {
    fn from_rule(rule: &'a Rule) -> Self {
        let code = rule.noqa_code().to_string();
        let (linter, _) = Linter::parse_code(&code).unwrap();
        let fix = rule.fixable().to_string();
        Self {
            name: rule.name().as_str(),
            code,
            linter: linter.name(),
            summary: rule.message_formats()[0],
            message_formats: rule.message_formats(),
            fix,
            fix_availability: rule.fixable(),
            explanation: rule.explanation(),
            preview: rule.is_preview(),
            status: rule.group(),
            source_location: SourceLocation {
                file: rule.file(),
                line: rule.line(),
            },
        }
    }
}

fn format_rule_text(rule: Rule) -> String {
    let mut output = String::new();
    let _ = write!(&mut output, "# {} ({})", rule.name(), rule.noqa_code());
    output.push('\n');
    output.push('\n');

    let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
    let _ = write!(
        &mut output,
        "Derived from the **{}** linter.",
        linter.name()
    );
    output.push('\n');
    output.push('\n');

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
            r"This rule is in preview and is not stable. The `--preview` flag is required for use.",
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
            let _ = write!(&mut output, "* {format}");
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
    }
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

/// The location of the rule's implementation in the Ruff source tree, relative to the repository
/// root.
///
/// For most rules this will point to the `#[derive(ViolationMetadata)]` line above the rule's
/// struct.
#[derive(Serialize)]
struct SourceLocation {
    file: &'static str,
    line: u32,
}
