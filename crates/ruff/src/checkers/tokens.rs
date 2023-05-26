//! Lint rules based on token traversal.

use ruff_text_size::TextRange;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use crate::lex::docstring_detection::StateMachine;
use crate::registry::{AsRule, Rule};
use crate::rules::ruff::rules::Context;
use crate::rules::{
    eradicate, flake8_commas, flake8_fixme, flake8_implicit_str_concat, flake8_pyi, flake8_quotes,
    flake8_todos, pycodestyle, pylint, pyupgrade, ruff,
};
use crate::settings::Settings;
use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::{Indexer, Locator};

use super::todo_directives::TodoDirective;

pub(crate) fn check_tokens(
    locator: &Locator,
    indexer: &Indexer,
    tokens: &[LexResult],
    settings: &Settings,
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

    // Combine flake8_todos and flake8_fixme so that we can reuse detected [`TodoDirective`]s.
    let enforce_todos = settings.rules.any_enabled(&[
        Rule::InvalidTodoTag,
        Rule::MissingTodoAuthor,
        Rule::MissingTodoLink,
        Rule::MissingTodoColon,
        Rule::MissingTodoDescription,
        Rule::InvalidTodoCapitalization,
        Rule::MissingSpaceAfterTodoColon,
        Rule::LineContainsFixme,
        Rule::LineContainsXxx,
        Rule::LineContainsTodo,
        Rule::LineContainsHack,
    ]);

    // RUF001, RUF002, RUF003
    if enforce_ambiguous_unicode_character {
        let mut state_machine = StateMachine::default();
        for &(ref tok, range) in tokens.iter().flatten() {
            let is_docstring = if enforce_ambiguous_unicode_character {
                state_machine.consume(tok)
            } else {
                false
            };

            if matches!(tok, Tok::String { .. } | Tok::Comment(_)) {
                diagnostics.extend(ruff::rules::ambiguous_unicode_character(
                    locator,
                    range,
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
                ));
            }
        }
    }

    // ERA001
    if enforce_commented_out_code {
        diagnostics.extend(eradicate::rules::commented_out_code(
            indexer, locator, settings,
        ));
    }

    // W605
    if enforce_invalid_escape_sequence {
        for (tok, range) in tokens.iter().flatten() {
            if matches!(tok, Tok::String { .. }) {
                diagnostics.extend(pycodestyle::rules::invalid_escape_sequence(
                    locator,
                    *range,
                    settings.rules.should_fix(Rule::InvalidEscapeSequence),
                ));
            }
        }
    }
    // PLE2510, PLE2512, PLE2513
    if enforce_invalid_string_character {
        for (tok, range) in tokens.iter().flatten() {
            if matches!(tok, Tok::String { .. }) {
                diagnostics.extend(
                    pylint::rules::invalid_string_characters(locator, *range)
                        .into_iter()
                        .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
                );
            }
        }
    }

    // E701, E702, E703
    if enforce_compound_statements {
        diagnostics.extend(
            pycodestyle::rules::compound_statements(tokens, settings)
                .into_iter()
                .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );
    }

    // Q001, Q002, Q003
    if enforce_quotes {
        diagnostics.extend(
            flake8_quotes::rules::from_tokens(tokens, locator, settings)
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
                locator,
            )
            .into_iter()
            .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );
    }

    // COM812, COM818, COM819
    if enforce_trailing_comma {
        diagnostics.extend(
            flake8_commas::rules::trailing_commas(tokens, locator, settings)
                .into_iter()
                .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );
    }

    // UP034
    if enforce_extraneous_parenthesis {
        diagnostics.extend(
            pyupgrade::rules::extraneous_parentheses(tokens, locator, settings).into_iter(),
        );
    }

    // PYI033
    if enforce_type_comment_in_stub && is_stub {
        diagnostics.extend(flake8_pyi::rules::type_comment_in_stub(indexer, locator));
    }

    // TD001, TD002, TD003, TD004, TD005, TD006, TD007
    // T001, T002, T003, T004
    if enforce_todos {
        // The TextRange of the comment, its position in comment_ranges, and the directive's
        // TextRange
        let mut other_directive_ranges: Vec<(TextRange, usize, TextRange)> = vec![];
        let mut flake8_fixme_directive_ranges: Vec<TextRange> = vec![];

        // Find all TodoDirectives
        for (i, comment_range) in indexer.comment_ranges().iter().enumerate() {
            let comment = locator.slice(*comment_range);
            let Some((directive, relative_offset)) = TodoDirective::from_comment(comment) else {
                continue;
            };

            let directive_range =
                TextRange::at(comment_range.start() + relative_offset, directive.len());

            // TODO, XXX, and FIXME directives are supported by flake8_todos. flake8_fixme supports
            // all 4 TodoDirective variants.
            if !matches!(directive, TodoDirective::Hack) {
                other_directive_ranges.push((*comment_range, i, directive_range));
            }
            flake8_fixme_directive_ranges.push(directive_range);
        }

        diagnostics.extend(
            flake8_todos::rules::todos(other_directive_ranges, indexer, locator, settings)
                .into_iter()
                .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );

        diagnostics.extend(
            flake8_fixme::rules::todos(flake8_fixme_directive_ranges, locator)
                .into_iter()
                .filter(|diagnostic| settings.rules.enabled(diagnostic.kind.rule())),
        );
    }

    diagnostics
}
