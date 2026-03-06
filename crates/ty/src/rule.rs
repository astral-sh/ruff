use std::fmt::Write as _;
use std::io::{self, BufWriter, Write};

use anyhow::Result;
use serde::{Serialize, Serializer, ser::SerializeSeq};

use ty_python_semantic::default_lint_registry;
use ty_python_semantic::lint::{Level, LintId, LintStatus};

use crate::args::HelpFormat;

#[derive(Serialize)]
struct RuleExplanation {
    name: String,
    summary: String,
    documentation: String,
    default_level: Level,
    status: LintStatus,
}

impl RuleExplanation {
    fn from_lint(lint: LintId) -> Self {
        Self {
            name: lint.name().to_string(),
            summary: lint.summary().to_owned(),
            documentation: lint.documentation(),
            default_level: lint.default_level(),
            status: *lint.status(),
        }
    }
}

fn format_rule_text(lint: LintId) -> String {
    let mut output = format!("# {}\n\n", lint.name());

    let status = match lint.status() {
        LintStatus::Preview { since } => format!("Preview (since {since})"),
        LintStatus::Stable { since } => format!("Stable (since {since})"),
        LintStatus::Deprecated { since, reason } => {
            format!("Deprecated (since {since}): {reason}")
        }
        LintStatus::Removed { since, reason } => format!("Removed (since {since}): {reason}"),
    };

    let _ = write!(
        output,
        "Default level: {} | {status}\n\n",
        lint.default_level()
    );

    output.push_str(lint.documentation().trim());

    output
}

/// Explain a single rule.
pub(crate) fn rule(name: &str, format: HelpFormat) -> Result<()> {
    let registry = default_lint_registry();
    let lint = registry.get(name).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut stdout = BufWriter::new(io::stdout().lock());
    match format {
        HelpFormat::Text => {
            writeln!(stdout, "{}", format_rule_text(lint))?;
        }
        HelpFormat::Json => {
            serde_json::to_writer_pretty(&mut stdout, &RuleExplanation::from_lint(lint))?;
            writeln!(stdout)?;
        }
    }
    Ok(())
}

/// Explain all rules.
pub(crate) fn rules(format: HelpFormat) -> Result<()> {
    let registry = default_lint_registry();
    let mut lints: Vec<LintId> = registry.lints().to_vec();
    lints.sort_by_key(|l| l.name());

    let mut stdout = BufWriter::new(io::stdout().lock());
    match format {
        HelpFormat::Text => {
            for lint in lints {
                writeln!(stdout, "{}", format_rule_text(lint))?;
                writeln!(stdout)?;
            }
        }
        HelpFormat::Json => {
            let mut serializer = serde_json::Serializer::pretty(stdout);
            let mut seq = serializer.serialize_seq(None)?;
            for lint in lints {
                seq.serialize_element(&RuleExplanation::from_lint(lint))?;
            }
            seq.end()?;
        }
    }
    Ok(())
}
