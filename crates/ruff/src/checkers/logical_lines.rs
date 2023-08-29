use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_python_codegen::Stylist;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::registry::{AsRule, Rule};
use crate::rules::pycodestyle::rules::logical_lines::{
    extraneous_whitespace, indentation, missing_whitespace, missing_whitespace_after_keyword,
    missing_whitespace_around_operator, space_after_comma, space_around_operator,
    whitespace_around_keywords, whitespace_around_named_parameter_equals,
    whitespace_before_comment, whitespace_before_parameters, LogicalLines, TokenFlags,
};
use crate::settings::Settings;

/// Return the amount of indentation, expanding tabs to the next multiple of 8.
fn expand_indent(line: &str) -> usize {
    let line = line.trim_end_matches(['\n', '\r']);

    let mut indent = 0;
    for c in line.bytes() {
        match c {
            b'\t' => indent = (indent / 8) * 8 + 8,
            b' ' => indent += 1,
            _ => break,
        }
    }

    indent
}

pub(crate) fn check_logical_lines(
    tokens: &[LexResult],
    locator: &Locator,
    stylist: &Stylist,
    settings: &Settings,
) -> Vec<Diagnostic> {
    let mut context = LogicalLinesContext::new(settings);

    let should_fix_missing_whitespace = settings.rules.should_fix(Rule::MissingWhitespace);
    let should_fix_whitespace_before_parameters =
        settings.rules.should_fix(Rule::WhitespaceBeforeParameters);
    let should_fix_whitespace_after_open_bracket =
        settings.rules.should_fix(Rule::WhitespaceAfterOpenBracket);
    let should_fix_whitespace_before_close_bracket = settings
        .rules
        .should_fix(Rule::WhitespaceBeforeCloseBracket);
    let should_fix_whitespace_before_punctuation =
        settings.rules.should_fix(Rule::WhitespaceBeforePunctuation);

    let mut prev_line = None;
    let mut prev_indent_level = None;
    let indent_char = stylist.indentation().as_char();

    for line in &LogicalLines::from_tokens(tokens, locator) {
        if line.flags().contains(TokenFlags::OPERATOR) {
            space_around_operator(&line, &mut context);
            whitespace_around_named_parameter_equals(&line, &mut context);
            missing_whitespace_around_operator(&line, &mut context);
            missing_whitespace(&line, should_fix_missing_whitespace, &mut context);
        }
        if line.flags().contains(TokenFlags::PUNCTUATION) {
            space_after_comma(&line, &mut context);
        }

        if line
            .flags()
            .intersects(TokenFlags::OPERATOR | TokenFlags::BRACKET | TokenFlags::PUNCTUATION)
        {
            extraneous_whitespace(
                &line,
                &mut context,
                should_fix_whitespace_after_open_bracket,
                should_fix_whitespace_before_close_bracket,
                should_fix_whitespace_before_punctuation,
            );
        }

        if line.flags().contains(TokenFlags::KEYWORD) {
            whitespace_around_keywords(&line, &mut context);
            missing_whitespace_after_keyword(&line, &mut context);
        }

        if line.flags().contains(TokenFlags::COMMENT) {
            whitespace_before_comment(&line, locator, &mut context);
        }

        if line.flags().contains(TokenFlags::BRACKET) {
            whitespace_before_parameters(
                &line,
                should_fix_whitespace_before_parameters,
                &mut context,
            );
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

        let indent_level = expand_indent(locator.slice(range));

        let indent_size = 4;

        for kind in indentation(
            &line,
            prev_line.as_ref(),
            indent_char,
            indent_level,
            prev_indent_level,
            indent_size,
        ) {
            if settings.rules.enabled(kind.rule()) {
                context.push(kind, range);
            }
        }

        if !line.is_comment_only() {
            prev_line = Some(line);
            prev_indent_level = Some(indent_level);
        }
    }
    context.diagnostics
}

#[derive(Debug, Clone)]
pub(crate) struct LogicalLinesContext<'a> {
    settings: &'a Settings,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> LogicalLinesContext<'a> {
    fn new(settings: &'a Settings) -> Self {
        Self {
            settings,
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn push<K: Into<DiagnosticKind>>(&mut self, kind: K, range: TextRange) {
        let kind = kind.into();
        if self.settings.rules.enabled(kind.rule()) {
            self.diagnostics.push(Diagnostic {
                kind,
                range,
                fix: None,
                parent: None,
            });
        }
    }

    pub(crate) fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        if self.settings.rules.enabled(diagnostic.kind.rule()) {
            self.diagnostics.push(diagnostic);
        }
    }
}
