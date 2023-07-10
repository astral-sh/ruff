//! Lint rules based on token traversal.

use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::{Indexer, Locator};

use crate::directives::TodoComment;
use crate::lex::docstring_detection::StateMachine;
use crate::registry::{AsRule, Rule};
use crate::rules::ruff::rules::Context;
use crate::rules::{
    eradicate, flake8_commas, flake8_fixme, flake8_implicit_str_concat, flake8_pyi, flake8_quotes,
    flake8_todos, pycodestyle, pylint, pyupgrade, ruff,
};
use crate::settings::Settings;

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
                ruff::rules::ambiguous_unicode_character(
                    &mut diagnostics,
                    locator,
                    range,
                    if tok.is_string() {
                        if is_docstring {
                            Context::Docstring
                        } else {
                            Context::String
                        }
                    } else {
                        Context::Comment
                    },
                    settings,
                );
            }
        }
    }

    // ERA001
    if enforce_commented_out_code {
        eradicate::rules::commented_out_code(&mut diagnostics, locator, indexer, settings);
    }

    // W605
    if enforce_invalid_escape_sequence {
        for (tok, range) in tokens.iter().flatten() {
            if tok.is_string() {
                pycodestyle::rules::invalid_escape_sequence(
                    &mut diagnostics,
                    locator,
                    *range,
                    settings.rules.should_fix(Rule::InvalidEscapeSequence),
                );
            }
        }
    }
    // PLE2510, PLE2512, PLE2513
    if enforce_invalid_string_character {
        for (tok, range) in tokens.iter().flatten() {
            if tok.is_string() {
                pylint::rules::invalid_string_characters(&mut diagnostics, *range, locator);
            }
        }
    }

    // E701, E702, E703
    if enforce_compound_statements {
        pycodestyle::rules::compound_statements(
            &mut diagnostics,
            tokens,
            locator,
            indexer,
            settings,
        );
    }

    // Q001, Q002, Q003
    if enforce_quotes {
        flake8_quotes::rules::from_tokens(&mut diagnostics, tokens, locator, settings);
    }

    // ISC001, ISC002
    if enforce_implicit_string_concatenation {
        flake8_implicit_str_concat::rules::implicit(
            &mut diagnostics,
            tokens,
            &settings.flake8_implicit_str_concat,
            locator,
        );
    }

    // COM812, COM818, COM819
    if enforce_trailing_comma {
        flake8_commas::rules::trailing_commas(&mut diagnostics, tokens, locator, settings);
    }

    // UP034
    if enforce_extraneous_parenthesis {
        pyupgrade::rules::extraneous_parentheses(&mut diagnostics, tokens, locator, settings);
    }

    // PYI033
    if enforce_type_comment_in_stub && is_stub {
        flake8_pyi::rules::type_comment_in_stub(&mut diagnostics, locator, indexer);
    }

    // TD001, TD002, TD003, TD004, TD005, TD006, TD007
    // T001, T002, T003, T004
    if enforce_todos {
        let todo_comments: Vec<TodoComment> = indexer
            .comment_ranges()
            .iter()
            .enumerate()
            .filter_map(|(i, comment_range)| {
                let comment = locator.slice(*comment_range);
                TodoComment::from_comment(comment, *comment_range, i)
            })
            .collect();

        flake8_todos::rules::todos(&mut diagnostics, &todo_comments, locator, indexer, settings);

        flake8_fixme::rules::todos(&mut diagnostics, &todo_comments);
    }

    diagnostics.retain(|diagnostic| settings.rules.enabled(diagnostic.kind.rule()));

    diagnostics
}
