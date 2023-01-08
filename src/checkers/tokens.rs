//! Lint rules based on token traversal.

use rustpython_parser::lexer::{LexResult, Tok};

use crate::lex::docstring_detection::StateMachine;
use crate::registry::{Diagnostic, RuleCode};
use crate::ruff::checks::Context;
use crate::settings::flags;
use crate::source_code_locator::SourceCodeLocator;
use crate::{eradicate, flake8_implicit_str_concat, flake8_quotes, pycodestyle, ruff, Settings};

pub fn check_tokens(
    locator: &SourceCodeLocator,
    tokens: &[LexResult],
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let enforce_ambiguous_unicode_character = settings.enabled.contains(&RuleCode::RUF001)
        || settings.enabled.contains(&RuleCode::RUF002)
        || settings.enabled.contains(&RuleCode::RUF003);
    let enforce_quotes = settings.enabled.contains(&RuleCode::Q000)
        || settings.enabled.contains(&RuleCode::Q001)
        || settings.enabled.contains(&RuleCode::Q002)
        || settings.enabled.contains(&RuleCode::Q003);
    let enforce_commented_out_code = settings.enabled.contains(&RuleCode::ERA001);
    let enforce_invalid_escape_sequence = settings.enabled.contains(&RuleCode::W605);
    let enforce_implicit_string_concatenation = settings.enabled.contains(&RuleCode::ISC001)
        || settings.enabled.contains(&RuleCode::ISC002);

    let mut state_machine = StateMachine::default();
    for &(start, ref tok, end) in tokens.iter().flatten() {
        let is_docstring = if enforce_ambiguous_unicode_character || enforce_quotes {
            state_machine.consume(tok)
        } else {
            false
        };

        // RUF001, RUF002, RUF003
        if enforce_ambiguous_unicode_character {
            if matches!(tok, Tok::String { .. } | Tok::Comment(_)) {
                diagnostics.extend(ruff::checks::ambiguous_unicode_character(
                    locator,
                    start,
                    end,
                    if matches!(tok, Tok::String { .. }) {
                        if is_docstring {
                            Context::Docstring
                        } else {
                            Context::String
                        }
                    } else {
                        Context::Comment
                    },
                    settings,
                    autofix,
                ));
            }
        }

        // flake8-quotes
        if enforce_quotes {
            if matches!(tok, Tok::String { .. }) {
                if let Some(diagnostic) = flake8_quotes::checks::quotes(
                    locator,
                    start,
                    end,
                    is_docstring,
                    &settings.flake8_quotes,
                ) {
                    if settings.enabled.contains(diagnostic.kind.code()) {
                        diagnostics.push(diagnostic);
                    }
                }
            }
        }

        // eradicate
        if enforce_commented_out_code {
            if matches!(tok, Tok::Comment(_)) {
                if let Some(diagnostic) =
                    eradicate::checks::commented_out_code(locator, start, end, settings, autofix)
                {
                    diagnostics.push(diagnostic);
                }
            }
        }

        // W605
        if enforce_invalid_escape_sequence {
            if matches!(tok, Tok::String { .. }) {
                diagnostics.extend(pycodestyle::checks::invalid_escape_sequence(
                    locator,
                    start,
                    end,
                    matches!(autofix, flags::Autofix::Enabled)
                        && settings.fixable.contains(&RuleCode::W605),
                ));
            }
        }
    }

    // ISC001, ISC002
    if enforce_implicit_string_concatenation {
        diagnostics.extend(
            flake8_implicit_str_concat::checks::implicit(tokens, locator)
                .into_iter()
                .filter(|diagnostic| settings.enabled.contains(diagnostic.kind.code())),
        );
    }

    diagnostics
}
