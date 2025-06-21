use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_parser::{TokenKind, Tokens};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::line_width::IndentWidth;
use crate::registry::Rule;
use crate::rules::pycodestyle::rules::logical_lines::{
    LogicalLines, TokenFlags, extraneous_whitespace, indentation, missing_whitespace,
    missing_whitespace_after_keyword, missing_whitespace_around_operator, redundant_backslash,
    space_after_comma, space_around_operator, whitespace_around_keywords,
    whitespace_around_named_parameter_equals, whitespace_before_comment,
    whitespace_before_parameters,
};
use crate::settings::LinterSettings;

use super::ast::LintContext;

/// Return the amount of indentation, expanding tabs to the next multiple of the settings' tab size.
pub(crate) fn expand_indent(line: &str, indent_width: IndentWidth) -> usize {
    let line = line.trim_end_matches(['\n', '\r']);

    let mut indent = 0;
    let tab_size = indent_width.as_usize();
    for c in line.bytes() {
        match c {
            b'\t' => indent = (indent / tab_size) * tab_size + tab_size,
            b' ' => indent += 1,
            _ => break,
        }
    }

    indent
}

pub(crate) fn check_logical_lines(
    tokens: &Tokens,
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
    settings: &LinterSettings,
    context: &LintContext,
) {
    let mut prev_line = None;
    let mut prev_indent_level = None;
    let indent_char = stylist.indentation().as_char();

    let enforce_space_around_operator = context.any_rule_enabled(&[
        Rule::MultipleSpacesBeforeOperator,
        Rule::MultipleSpacesAfterOperator,
        Rule::TabBeforeOperator,
        Rule::TabAfterOperator,
    ]);
    let enforce_whitespace_around_named_parameter_equals = context.any_rule_enabled(&[
        Rule::UnexpectedSpacesAroundKeywordParameterEquals,
        Rule::MissingWhitespaceAroundParameterEquals,
    ]);
    let enforce_missing_whitespace_around_operator = context.any_rule_enabled(&[
        Rule::MissingWhitespaceAroundOperator,
        Rule::MissingWhitespaceAroundArithmeticOperator,
        Rule::MissingWhitespaceAroundBitwiseOrShiftOperator,
        Rule::MissingWhitespaceAroundModuloOperator,
    ]);
    let enforce_missing_whitespace = context.is_rule_enabled(Rule::MissingWhitespace);
    let enforce_space_after_comma =
        context.any_rule_enabled(&[Rule::MultipleSpacesAfterComma, Rule::TabAfterComma]);
    let enforce_extraneous_whitespace = context.any_rule_enabled(&[
        Rule::WhitespaceAfterOpenBracket,
        Rule::WhitespaceBeforeCloseBracket,
        Rule::WhitespaceBeforePunctuation,
    ]);
    let enforce_whitespace_around_keywords = context.any_rule_enabled(&[
        Rule::MultipleSpacesAfterKeyword,
        Rule::MultipleSpacesBeforeKeyword,
        Rule::TabAfterKeyword,
        Rule::TabBeforeKeyword,
    ]);
    let enforce_missing_whitespace_after_keyword =
        context.is_rule_enabled(Rule::MissingWhitespaceAfterKeyword);
    let enforce_whitespace_before_comment = context.any_rule_enabled(&[
        Rule::TooFewSpacesBeforeInlineComment,
        Rule::NoSpaceAfterInlineComment,
        Rule::NoSpaceAfterBlockComment,
        Rule::MultipleLeadingHashesForBlockComment,
    ]);
    let enforce_whitespace_before_parameters =
        context.is_rule_enabled(Rule::WhitespaceBeforeParameters);
    let enforce_redundant_backslash = context.is_rule_enabled(Rule::RedundantBackslash);
    let enforce_indentation = context.any_rule_enabled(&[
        Rule::IndentationWithInvalidMultiple,
        Rule::NoIndentedBlock,
        Rule::UnexpectedIndentation,
        Rule::IndentationWithInvalidMultipleComment,
        Rule::NoIndentedBlockComment,
        Rule::UnexpectedIndentationComment,
        Rule::OverIndented,
    ]);

    for line in &LogicalLines::from_tokens(tokens, locator) {
        if line.flags().contains(TokenFlags::OPERATOR) {
            if enforce_space_around_operator {
                space_around_operator(&line, context);
            }

            if enforce_whitespace_around_named_parameter_equals {
                whitespace_around_named_parameter_equals(&line, context);
            }

            if enforce_missing_whitespace_around_operator {
                missing_whitespace_around_operator(&line, context);
            }

            if enforce_missing_whitespace {
                missing_whitespace(&line, context);
            }
        }

        if line.flags().contains(TokenFlags::PUNCTUATION) && enforce_space_after_comma {
            space_after_comma(&line, context);
        }

        if line
            .flags()
            .intersects(TokenFlags::OPERATOR | TokenFlags::BRACKET | TokenFlags::PUNCTUATION)
            && enforce_extraneous_whitespace
        {
            extraneous_whitespace(&line, context);
        }

        if line.flags().contains(TokenFlags::KEYWORD) {
            if enforce_whitespace_around_keywords {
                whitespace_around_keywords(&line, context);
            }

            if enforce_missing_whitespace_after_keyword {
                missing_whitespace_after_keyword(&line, context);
            }
        }

        if line.flags().contains(TokenFlags::COMMENT) && enforce_whitespace_before_comment {
            whitespace_before_comment(&line, locator, context);
        }

        if line.flags().contains(TokenFlags::BRACKET) {
            if enforce_whitespace_before_parameters {
                whitespace_before_parameters(&line, context);
            }

            if enforce_redundant_backslash {
                redundant_backslash(&line, locator, indexer, context);
            }
        }

        // Extract the indentation level.
        let Some(first_token) = line.first_token() else {
            continue;
        };

        let range = if first_token.kind() == TokenKind::Indent {
            first_token.range()
        } else {
            TextRange::new(locator.line_start(first_token.start()), first_token.start())
        };

        let indent_level = expand_indent(locator.slice(range), settings.tab_size);

        let indent_size = 4;

        if enforce_indentation {
            indentation(
                &line,
                prev_line.as_ref(),
                indent_char,
                indent_level,
                prev_indent_level,
                indent_size,
                range,
                context,
            );
        }

        if !line.is_comment_only() {
            prev_line = Some(line);
            prev_indent_level = Some(indent_level);
        }
    }
}
