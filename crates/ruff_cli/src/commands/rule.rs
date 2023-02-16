use std::io::{self, BufWriter, Write};

use anyhow::Result;
use colored::control::SHOULD_COLORIZE;
use mdcat::terminal::{TerminalProgram, TerminalSize};
use mdcat::{Environment, ResourceAccess, Settings};
use pulldown_cmark::{Options, Parser};
use serde::Serialize;
use syntect::parsing::SyntaxSet;

use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff::AutofixAvailability;

use crate::args::HelpFormat;

#[derive(Serialize)]
struct Explanation<'a> {
    code: &'a str,
    linter: &'a str,
    summary: &'a str,
}

/// Explain a `Rule` to the user.
pub fn rule(rule: &Rule, format: HelpFormat) -> Result<()> {
    let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
    let mut stdout = BufWriter::new(io::stdout().lock());
    let mut output = String::new();

    match format {
        HelpFormat::Text | HelpFormat::Pretty => {
            output.push_str(&format!("# {} ({})", rule.as_ref(), rule.noqa_code()));
            output.push('\n');
            output.push('\n');

            let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
            output.push_str(&format!("Derived from the **{}** linter.", linter.name()));
            output.push('\n');
            output.push('\n');

            if let Some(autofix) = rule.autofixable() {
                output.push_str(match autofix.available {
                    AutofixAvailability::Sometimes => "Autofix is sometimes available.",
                    AutofixAvailability::Always => "Autofix is always available.",
                });
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

    match format {
        HelpFormat::Json | HelpFormat::Text => {
            writeln!(stdout, "{output}")?;
        }
        HelpFormat::Pretty => {
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
                resource_access: ResourceAccess::LocalOnly,
                syntax_set: SyntaxSet::load_defaults_newlines(),
                terminal_capabilities: terminal.capabilities(),
                terminal_size: TerminalSize::detect().unwrap_or_default(),
            };

            mdcat::push_tty(settings, env, &mut stdout, parser)?;
        }
    };

    Ok(())
}
