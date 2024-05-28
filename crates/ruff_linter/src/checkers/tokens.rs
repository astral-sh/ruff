//! Lint rules based on token traversal.

use std::path::Path;

use ruff_notebook::CellOffsets;
use ruff_python_ast::{ModModule, PySourceType};
use ruff_python_codegen::Stylist;

use ruff_diagnostics::Diagnostic;
use ruff_python_index::Indexer;
use ruff_python_parser::{Program, Tokens};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::directives::TodoComment;
use crate::registry::{AsRule, Rule};
use crate::rules::pycodestyle::rules::BlankLinesChecker;
use crate::rules::{
    eradicate, flake8_commas, flake8_executable, flake8_fixme, flake8_implicit_str_concat,
    flake8_pyi, flake8_todos, pycodestyle, pygrep_hooks, pylint, pyupgrade, ruff,
};
use crate::settings::LinterSettings;

#[allow(clippy::too_many_arguments)]
pub(crate) fn check_tokens(
    program: &Program<ModModule>,
    path: &Path,
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
    settings: &LinterSettings,
    source_type: PySourceType,
    cell_offsets: Option<&CellOffsets>,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    if settings.rules.any_enabled(&[
        Rule::BlankLineBetweenMethods,
        Rule::BlankLinesTopLevel,
        Rule::TooManyBlankLines,
        Rule::BlankLineAfterDecorator,
        Rule::BlankLinesAfterFunctionOrClass,
        Rule::BlankLinesBeforeNestedDefinition,
    ]) {
        BlankLinesChecker::new(locator, stylist, settings, source_type, cell_offsets)
            .check_lines(program.tokens(), &mut diagnostics);
    }

    if settings.rules.enabled(Rule::BlanketTypeIgnore) {
        pygrep_hooks::rules::blanket_type_ignore(
            &mut diagnostics,
            program.comment_ranges(),
            locator,
        );
    }

    if settings.rules.enabled(Rule::EmptyComment) {
        pylint::rules::empty_comments(&mut diagnostics, program.comment_ranges(), locator);
    }

    if settings
        .rules
        .enabled(Rule::AmbiguousUnicodeCharacterComment)
    {
        for range in program.comment_ranges() {
            ruff::rules::ambiguous_unicode_character_comment(
                &mut diagnostics,
                locator,
                *range,
                settings,
            );
        }
    }

    if settings.rules.enabled(Rule::CommentedOutCode) {
        eradicate::rules::commented_out_code(
            &mut diagnostics,
            locator,
            program.comment_ranges(),
            settings,
        );
    }

    if settings.rules.enabled(Rule::UTF8EncodingDeclaration) {
        pyupgrade::rules::unnecessary_coding_comment(
            &mut diagnostics,
            locator,
            indexer,
            program.comment_ranges(),
        );
    }

    if settings.rules.enabled(Rule::TabIndentation) {
        pycodestyle::rules::tab_indentation(&mut diagnostics, locator, indexer);
    }

    if settings.rules.any_enabled(&[
        Rule::InvalidCharacterBackspace,
        Rule::InvalidCharacterSub,
        Rule::InvalidCharacterEsc,
        Rule::InvalidCharacterNul,
        Rule::InvalidCharacterZeroWidthSpace,
    ]) {
        for token in program.tokens().up_to_first_unknown() {
            pylint::rules::invalid_string_characters(
                &mut diagnostics,
                token.kind(),
                token.range(),
                locator,
            );
        }
    }

    if settings.rules.any_enabled(&[
        Rule::MultipleStatementsOnOneLineColon,
        Rule::MultipleStatementsOnOneLineSemicolon,
        Rule::UselessSemicolon,
    ]) {
        pycodestyle::rules::compound_statements(
            &mut diagnostics,
            program.tokens(),
            locator,
            indexer,
            source_type,
            cell_offsets,
        );
    }

    if settings.rules.any_enabled(&[
        Rule::SingleLineImplicitStringConcatenation,
        Rule::MultiLineImplicitStringConcatenation,
    ]) {
        flake8_implicit_str_concat::rules::implicit(
            &mut diagnostics,
            program.tokens(),
            settings,
            locator,
            indexer,
        );
    }

    if settings.rules.any_enabled(&[
        Rule::MissingTrailingComma,
        Rule::TrailingCommaOnBareTuple,
        Rule::ProhibitedTrailingComma,
    ]) {
        flake8_commas::rules::trailing_commas(&mut diagnostics, program.tokens(), locator, indexer);
    }

    if settings.rules.enabled(Rule::ExtraneousParentheses) {
        pyupgrade::rules::extraneous_parentheses(&mut diagnostics, program.tokens(), locator);
    }

    if source_type.is_stub() && settings.rules.enabled(Rule::TypeCommentInStub) {
        flake8_pyi::rules::type_comment_in_stub(
            &mut diagnostics,
            locator,
            program.comment_ranges(),
        );
    }

    if settings.rules.any_enabled(&[
        Rule::ShebangNotExecutable,
        Rule::ShebangMissingExecutableFile,
        Rule::ShebangLeadingWhitespace,
        Rule::ShebangNotFirstLine,
        Rule::ShebangMissingPython,
    ]) {
        flake8_executable::rules::from_tokens(
            &mut diagnostics,
            path,
            locator,
            program.comment_ranges(),
        );
    }

    if settings.rules.any_enabled(&[
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
        let todo_comments: Vec<TodoComment> = program
            .comment_ranges()
            .iter()
            .enumerate()
            .filter_map(|(i, comment_range)| {
                let comment = locator.slice(*comment_range);
                TodoComment::from_comment(comment, *comment_range, i)
            })
            .collect();
        flake8_todos::rules::todos(
            &mut diagnostics,
            &todo_comments,
            locator,
            program.comment_ranges(),
        );
        flake8_fixme::rules::todos(&mut diagnostics, &todo_comments);
    }

    if settings.rules.enabled(Rule::TooManyNewlinesAtEndOfFile) {
        pycodestyle::rules::too_many_newlines_at_end_of_file(&mut diagnostics, program.tokens());
    }

    diagnostics.retain(|diagnostic| settings.rules.enabled(diagnostic.kind.rule()));

    diagnostics
}
