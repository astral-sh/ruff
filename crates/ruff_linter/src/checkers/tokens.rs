//! Lint rules based on token traversal.

use std::path::Path;

use ruff_notebook::CellOffsets;
use ruff_python_ast::PySourceType;
use ruff_python_ast::token::Tokens;
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;

use crate::Locator;
use crate::directives::TodoComment;
use crate::registry::Rule;
use crate::rules::pycodestyle::rules::BlankLinesChecker;
use crate::rules::{
    eradicate, flake8_commas, flake8_executable, flake8_fixme, flake8_implicit_str_concat,
    flake8_pyi, flake8_todos, pycodestyle, pygrep_hooks, pylint, pyupgrade, ruff,
};

use super::ast::LintContext;

#[expect(clippy::too_many_arguments)]
pub(crate) fn check_tokens(
    tokens: &Tokens,
    path: &Path,
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
    source_type: PySourceType,
    cell_offsets: Option<&CellOffsets>,
    context: &mut LintContext,
) {
    let comment_ranges = indexer.comment_ranges();

    if context.any_rule_enabled(&[
        Rule::BlankLineBetweenMethods,
        Rule::BlankLinesTopLevel,
        Rule::TooManyBlankLines,
        Rule::BlankLineAfterDecorator,
        Rule::BlankLinesAfterFunctionOrClass,
        Rule::BlankLinesBeforeNestedDefinition,
    ]) {
        BlankLinesChecker::new(locator, stylist, source_type, cell_offsets, context)
            .check_lines(tokens);
    }

    if context.is_rule_enabled(Rule::BlanketTypeIgnore) {
        pygrep_hooks::rules::blanket_type_ignore(context, comment_ranges, locator);
    }

    if context.is_rule_enabled(Rule::EmptyComment) {
        pylint::rules::empty_comments(context, comment_ranges, locator, indexer);
    }

    if context.is_rule_enabled(Rule::AmbiguousUnicodeCharacterComment) {
        for range in comment_ranges {
            ruff::rules::ambiguous_unicode_character_comment(context, locator, range);
        }
    }

    if context.is_rule_enabled(Rule::CommentedOutCode) {
        eradicate::rules::commented_out_code(context, locator, comment_ranges);
    }

    if context.is_rule_enabled(Rule::UTF8EncodingDeclaration) {
        pyupgrade::rules::unnecessary_coding_comment(context, locator, comment_ranges);
    }

    if context.is_rule_enabled(Rule::TabIndentation) {
        pycodestyle::rules::tab_indentation(context, locator, indexer);
    }

    if context.any_rule_enabled(&[
        Rule::InvalidCharacterBackspace,
        Rule::InvalidCharacterSub,
        Rule::InvalidCharacterEsc,
        Rule::InvalidCharacterNul,
        Rule::InvalidCharacterZeroWidthSpace,
    ]) {
        for token in tokens {
            pylint::rules::invalid_string_characters(context, token, locator);
        }
    }

    if context.any_rule_enabled(&[
        Rule::MultipleStatementsOnOneLineColon,
        Rule::MultipleStatementsOnOneLineSemicolon,
        Rule::UselessSemicolon,
    ]) {
        pycodestyle::rules::compound_statements(
            context,
            tokens,
            locator,
            indexer,
            source_type,
            cell_offsets,
        );
    }

    if context.any_rule_enabled(&[
        Rule::SingleLineImplicitStringConcatenation,
        Rule::MultiLineImplicitStringConcatenation,
    ]) {
        flake8_implicit_str_concat::rules::implicit(context, tokens, locator, indexer);
    }

    if context.any_rule_enabled(&[
        Rule::MissingTrailingComma,
        Rule::TrailingCommaOnBareTuple,
        Rule::ProhibitedTrailingComma,
    ]) {
        flake8_commas::rules::trailing_commas(context, tokens, locator, indexer);
    }

    if context.is_rule_enabled(Rule::ExtraneousParentheses) {
        pyupgrade::rules::extraneous_parentheses(context, tokens, locator);
    }

    if source_type.is_stub() && context.is_rule_enabled(Rule::TypeCommentInStub) {
        flake8_pyi::rules::type_comment_in_stub(context, locator, comment_ranges);
    }

    if context.any_rule_enabled(&[
        Rule::ShebangNotExecutable,
        Rule::ShebangMissingExecutableFile,
        Rule::ShebangLeadingWhitespace,
        Rule::ShebangNotFirstLine,
        Rule::ShebangMissingPython,
    ]) {
        flake8_executable::rules::from_tokens(context, path, locator, comment_ranges);
    }

    if context.any_rule_enabled(&[
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
    ]) {
        let todo_comments: Vec<TodoComment> = comment_ranges
            .iter()
            .enumerate()
            .filter_map(|(i, comment_range)| {
                let comment = locator.slice(*comment_range);
                TodoComment::from_comment(comment, *comment_range, i)
            })
            .collect();
        flake8_todos::rules::todos(context, &todo_comments, locator, comment_ranges);
        flake8_fixme::rules::todos(context, &todo_comments);
    }

    if context.is_rule_enabled(Rule::TooManyNewlinesAtEndOfFile) {
        pycodestyle::rules::too_many_newlines_at_end_of_file(context, tokens, cell_offsets);
    }
}
