use std::io::{self, BufWriter, Write};

use anyhow::Result;
use colored::control::SHOULD_COLORIZE;
use pulldown_cmark::{Options, Parser};
use pulldown_cmark_mdcat::resources::NoopResourceHandler;
use pulldown_cmark_mdcat::terminal::{TerminalProgram, TerminalSize};
use pulldown_cmark_mdcat::{push_tty, Environment, Settings, Theme};
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use strum::IntoEnumIterator;
use syntect::parsing::SyntaxSet;

use ruff_diagnostics::FixAvailability;
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
    explanation: Option<&'a str>,
    preview: bool,
}

impl<'a> Explanation<'a> {
    fn from_rule(rule: &'a Rule) -> Self {
        let code = rule.noqa_code().to_string();
        let (linter, _) = Linter::parse_code(&code).unwrap();
        let fix = rule.fixable().to_string();
        Self {
            name: rule.as_ref(),
            code,
            linter: linter.name(),
            summary: rule.message_formats()[0],
            message_formats: rule.message_formats(),
            fix,
            explanation: rule.explanation(),
            preview: rule.is_preview() || rule.is_nursery(),
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

    let fix_availability = rule.fixable();
    if matches!(
        fix_availability,
        FixAvailability::Always | FixAvailability::Sometimes
    ) {
        output.push_str(&fix_availability.to_string());
        output.push('\n');
        output.push('\n');
    }

    if rule.is_preview() || rule.is_nursery() {
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

fn print_rule_text<W: Write>(writer: &mut W, rule: Rule) -> Result<()> {
    let output = format_rule_text(rule);

    let parser = Parser::new_ext(
        &output,
        Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
    );

    let cwd = std::env::current_dir()?;
    let env = &Environment::for_local_directory(&cwd)?;

    let terminal = if SHOULD_COLORIZE.should_colorize() {
        TerminalProgram::detect()
    } else {
        TerminalProgram::Dumb
    };

    let settings = &Settings {
        terminal_capabilities: terminal.capabilities(),
        terminal_size: TerminalSize::detect().unwrap_or_default(),
        syntax_set: &SyntaxSet::load_defaults_newlines(),
        theme: Theme::default(),
    };

    push_tty(settings, env, &NoopResourceHandler {}, writer, parser)?;
    Ok(())
}

/// Explain a `Rule` to the user.
pub(crate) fn rule(rule: Rule, format: HelpFormat) -> Result<()> {
    let mut stdout = BufWriter::new(io::stdout().lock());
    match format {
        HelpFormat::Pretty => {
            print_rule_text(&mut stdout, rule)?;
        }
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
        HelpFormat::Pretty => {
            for rule in Rule::iter() {
                print_rule_text(&mut stdout, rule)?;
                writeln!(stdout)?;
            }
        }
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
