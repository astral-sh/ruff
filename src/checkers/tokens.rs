//! Lint rules based on token traversal.

use rustpython_parser::lexer::{LexResult, Tok};

use crate::checks::{Check, CheckCode};
use crate::lex::docstring_detection::StateMachine;
use crate::ruff::checks::Context;
use crate::settings::flags;
use crate::source_code_locator::SourceCodeLocator;
use crate::{eradicate, flake8_implicit_str_concat, flake8_quotes, pycodestyle, ruff, Settings};

pub fn check_tokens(
    locator: &SourceCodeLocator,
    tokens: &[LexResult],
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    let enforce_ambiguous_unicode_character = settings.enabled.contains(&CheckCode::RUF001)
        || settings.enabled.contains(&CheckCode::RUF002)
        || settings.enabled.contains(&CheckCode::RUF003);
    let enforce_quotes = settings.enabled.contains(&CheckCode::Q000)
        || settings.enabled.contains(&CheckCode::Q001)
        || settings.enabled.contains(&CheckCode::Q002)
        || settings.enabled.contains(&CheckCode::Q003);
    let enforce_commented_out_code = settings.enabled.contains(&CheckCode::ERA001);
    let enforce_invalid_escape_sequence = settings.enabled.contains(&CheckCode::W605);
    let enforce_implicit_string_concatenation = settings.enabled.contains(&CheckCode::ISC001)
        || settings.enabled.contains(&CheckCode::ISC002);

    let mut state_machine = StateMachine::default();
    for &(start, ref tok, end) in tokens.iter().flatten() {
        let is_docstring = if enforce_ambiguous_unicode_character || enforce_quotes {
            state_machine.consume(tok)
        } else {
            false
        };

        // RUF001, RUF002, RUF003
        if enforce_ambiguous_unicode_character {
            if matches!(tok, Tok::String { .. } | Tok::Comment) {
                checks.extend(ruff::checks::ambiguous_unicode_character(
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
                if let Some(check) = flake8_quotes::checks::quotes(
                    locator,
                    start,
                    end,
                    is_docstring,
                    &settings.flake8_quotes,
                ) {
                    if settings.enabled.contains(check.kind.code()) {
                        checks.push(check);
                    }
                }
            }
        }

        // eradicate
        if enforce_commented_out_code {
            if matches!(tok, Tok::Comment) {
                if let Some(check) =
                    eradicate::checks::commented_out_code(locator, start, end, settings, autofix)
                {
                    checks.push(check);
                }
            }
        }

        // W605
        if enforce_invalid_escape_sequence {
            if matches!(tok, Tok::String { .. }) {
                checks.extend(pycodestyle::checks::invalid_escape_sequence(
                    locator,
                    start,
                    end,
                    matches!(autofix, flags::Autofix::Enabled)
                        && settings.fixable.contains(&CheckCode::W605),
                ));
            }
        }
    }

    // ISC001, ISC002
    if enforce_implicit_string_concatenation {
        checks.extend(
            flake8_implicit_str_concat::checks::implicit(tokens, locator)
                .into_iter()
                .filter(|check| settings.enabled.contains(check.kind.code())),
        );
    }

    checks
}
