//! Lint rules based on token traversal.

use rustpython_parser::lexer::{LexResult, Tok};

use crate::autofix::fixer;
use crate::checks::{Check, CheckCode};
use crate::flake8_quotes::docstring_detection::StateMachine;
use crate::source_code_locator::SourceCodeLocator;
use crate::{flake8_quotes, pycodestyle, rules, Settings};

pub fn check_tokens(
    checks: &mut Vec<Check>,
    locator: &SourceCodeLocator,
    tokens: &[LexResult],
    settings: &Settings,
    autofix: &fixer::Mode,
) {
    let enforce_ambiguous_unicode_character = settings.enabled.contains(&CheckCode::X001);
    let enforce_invalid_escape_sequence = settings.enabled.contains(&CheckCode::W605);
    let enforce_quotes = settings.enabled.contains(&CheckCode::Q000)
        | settings.enabled.contains(&CheckCode::Q001)
        | settings.enabled.contains(&CheckCode::Q002)
        | settings.enabled.contains(&CheckCode::Q003);

    let mut state_machine: StateMachine = Default::default();
    for (start, tok, end) in tokens.iter().flatten() {
        // X001
        if enforce_ambiguous_unicode_character {
            if matches!(tok, Tok::String { .. }) {
                checks.extend(rules::checks::ambiguous_unicode_character(
                    locator,
                    start,
                    end,
                    autofix.patch(),
                ));
            }
        }

        // W605
        if enforce_invalid_escape_sequence {
            if matches!(tok, Tok::String { .. }) {
                checks.extend(pycodestyle::checks::invalid_escape_sequence(
                    locator, start, end,
                ));
            }
        }

        // flake8-quotes
        if enforce_quotes {
            let is_docstring = state_machine.consume(tok);
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
    }
}
