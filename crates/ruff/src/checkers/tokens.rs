//! Lint rules based on token traversal.

use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use crate::lex::docstring_detection::StateMachine;
use crate::registry::{AsRule, Rule};
use crate::rules::ruff::rules::Context;
use crate::rules::{
    eradicate, flake8_commas, flake8_implicit_str_concat, flake8_pyi, flake8_quotes, pycodestyle,
    pylint, pyupgrade, ruff,
};
use crate::settings::{flags, Settings};
use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::Locator;

pub fn check_tokens(
    locator: &Locator,
    tokens: &[LexResult],
    settings: &Settings,
    autofix: flags::Autofix,
    is_stub: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let enforce_ambiguous_unicode_character = settings.rules.any_enabled(&[
        Rule::AmbiguousUnicodeCharacterString,
        Rule::AmbiguousUnicodeCharacterDocstring,
        Rule::AmbiguousUnicodeCharacterComment,
    ]);
    let enforce_invalid_string_character = settings.rules.any_enabled(&[
        Rule::InvalidCharacterBackspace,
        Rule::InvalidCharacterSub,
        Rule::InvalidCharacterEsc,
        Rule::InvalidCharacterNul,
        Rule::InvalidCharacterZeroWidthSpace,
    ]);
    let enforce_quotes = settings.rules.any_enabled(&[
        Rule::BadQuotesInlineString,
        Rule::BadQuotesMultilineString,
        Rule::BadQuotesDocstring,
        Rule::AvoidableEscapedQuote,
    ]);
    let enforce_commented_out_code = settings.rules.enabled(Rule::CommentedOutCode);
    let enforce_compound_statements = settings.rules.any_enabled(&[
        Rule::MultipleStatementsOnOneLineColon,
        Rule::MultipleStatementsOnOneLineSemicolon,
        Rule::UselessSemicolon,
    ]);
    let enforce_invalid_escape_sequence = settings.rules.enabled(Rule::InvalidEscapeSequence);
    let enforce_implicit_string_concatenation = settings.rules.any_enabled(&[
        Rule::SingleLineImplicitStringConcatenation,
        Rule::MultiLineImplicitStringConcatenation,
    ]);

    let enforce_trailing_comma = settings.rules.any_enabled(&[
        Rule::MissingTrailingComma,
        Rule::TrailingCommaOnBareTuple,
        Rule::ProhibitedTrailingComma,
    ]);
    let enforce_extraneous_parenthesis = settings.rules.enabled(Rule::ExtraneousParentheses);
    let enforce_type_comment_in_stub = settings.rules.enabled(Rule::TypeCommentInStub);

    // RUF001, RUF002, RUF003
    if enforce_ambiguous_unicode_character {
        let mut state_machine = StateMachine::default();
        for &(start, ref tok, end) in tokens.iter().flatten() {
            let is_docstring = if enforce_ambiguous_unicode_character {
                state_machine.consume(tok)
            } else {
                false
            };

            if matches!(tok, Tok::String { .. } | Tok::Comment(_)) {
                diagnostics.extend(ruff::rules::ambiguous_unicode_character(
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
    }

    // ERA001
    if enforce_commented_out_code {
        for (start, tok, end) in tokens.iter().flatten() {
            if matches!(tok, Tok::Comment(_)) {
                if let Some(diagnostic) =
                    eradicate::rules::commented_out_code(locator, *start, *end, settings, autofix)
                {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }

    // W605
    if enforce_invalid_escape_sequence {
        for (start, tok, end) in tokens.iter().flatten() {
            if matches!(tok, Tok::String { .. }) {
                diagnostics.extend(pycodestyle::rules::invalid_escape_sequence(
                    locator,
                    *start,
                    *end,
                    autofix.into() && settings.rules.should_fix(Rule::InvalidEscapeSequence),
                ));
            }
        }
    }
    // PLE2510, PLE2512, PLE2513
    if enforce_invalid_string_character {
        for (start, tok, end) in tokens.iter().flatten() {
            if matches!(tok, Tok::String { .. }) {
                diagnostics.extend(
                    pylint::rules::invalid_string_characters(locator, *start, *end, autofix.into())
                        .into_iter()
                        .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
                );
            }
        }
    }

    // E701, E702, E703
    if enforce_compound_statements {
        diagnostics.extend(
            pycodestyle::rules::compound_statements(tokens, settings, autofix)
                .into_iter()
                .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );
    }

    // Q001, Q002, Q003
    if enforce_quotes {
        diagnostics.extend(
            flake8_quotes::rules::from_tokens(tokens, locator, settings, autofix)
                .into_iter()
                .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );
    }

    // ISC001, ISC002
    if enforce_implicit_string_concatenation {
        diagnostics.extend(
            flake8_implicit_str_concat::rules::implicit(
                tokens,
                &settings.flake8_implicit_str_concat,
            )
            .into_iter()
            .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );
    }

    // COM812, COM818, COM819
    if enforce_trailing_comma {
        diagnostics.extend(
            flake8_commas::rules::trailing_commas(tokens, locator, settings, autofix)
                .into_iter()
                .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );
    }

    // UP034
    if enforce_extraneous_parenthesis {
        diagnostics.extend(
            pyupgrade::rules::extraneous_parentheses(tokens, locator, settings, autofix)
                .into_iter(),
        );
    }

    // PYI033
    if enforce_type_comment_in_stub && is_stub {
        diagnostics.extend(flake8_pyi::rules::type_comment_in_stub(tokens));
    }

    diagnostics
}
