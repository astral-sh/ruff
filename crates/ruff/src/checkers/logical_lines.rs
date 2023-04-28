use ruff_text_size::TextRange;
use rustpython_parser::lexer::LexResult;

use ruff_diagnostics::{Diagnostic, DiagnosticKind, Fix};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::token_kind::TokenKind;

use crate::registry::{AsRule, Rule};
use crate::rules::pycodestyle::rules::logical_lines::{
    extraneous_whitespace, indentation, missing_whitespace, missing_whitespace_after_keyword,
    missing_whitespace_around_operator, space_around_operator, whitespace_around_keywords,
    whitespace_around_named_parameter_equals, whitespace_before_comment,
    whitespace_before_parameters, LogicalLines, TokenFlags,
};
use crate::settings::{flags, Settings};

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

pub fn check_logical_lines(
    tokens: &[LexResult],
    locator: &Locator,
    stylist: &Stylist,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut context = LogicalLinesContext::new(settings);

    #[cfg(feature = "logical_lines")]
    let should_fix_missing_whitespace =
        autofix.into() && settings.rules.should_fix(Rule::MissingWhitespace);

    #[cfg(not(feature = "logical_lines"))]
    let should_fix_missing_whitespace = false;

    #[cfg(feature = "logical_lines")]
    let should_fix_whitespace_before_parameters =
        autofix.into() && settings.rules.should_fix(Rule::WhitespaceBeforeParameters);

    #[cfg(not(feature = "logical_lines"))]
    let should_fix_whitespace_before_parameters = false;

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

        if line
            .flags()
            .contains(TokenFlags::OPERATOR | TokenFlags::PUNCTUATION)
        {
            extraneous_whitespace(&line, &mut context);
        }
        if line.flags().contains(TokenFlags::KEYWORD) {
            whitespace_around_keywords(&line, &mut context);
            missing_whitespace_after_keyword(&line, &mut context);
        }

        if line.flags().contains(TokenFlags::COMMENT) {
            whitespace_before_comment(&line, locator, prev_line.is_none(), &mut context);
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

    pub fn push<K: Into<DiagnosticKind>>(&mut self, kind: K, range: TextRange) {
        let kind = kind.into();
        if self.settings.rules.enabled(kind.rule()) {
            self.diagnostics.push(Diagnostic {
                kind,
                range,
                fix: Fix::empty(),
                parent: None,
            });
        }
    }

    pub fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        if self.settings.rules.enabled(diagnostic.kind.rule()) {
            self.diagnostics.push(diagnostic);
        }
    }
}

#[cfg(test)]
mod tests {
    use rustpython_parser::lexer::LexResult;
    use rustpython_parser::{lexer, Mode};

    use crate::rules::pycodestyle::rules::logical_lines::LogicalLines;
    use ruff_python_ast::source_code::Locator;

    #[test]
    fn split_logical_lines() {
        let contents = r#"
x = 1
y = 2
z = x + 1"#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = LogicalLines::from_tokens(&lxr, &locator)
            .into_iter()
            .map(|line| line.text_trimmed().to_string())
            .collect();
        let expected = vec![
            "x = 1".to_string(),
            "y = 2".to_string(),
            "z = x + 1".to_string(),
        ];
        assert_eq!(actual, expected);

        let contents = r#"
x = [
  1,
  2,
  3,
]
y = 2
z = x + 1"#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = LogicalLines::from_tokens(&lxr, &locator)
            .into_iter()
            .map(|line| line.text_trimmed().to_string())
            .collect();
        let expected = vec![
            "x = [\n  1,\n  2,\n  3,\n]".to_string(),
            "y = 2".to_string(),
            "z = x + 1".to_string(),
        ];
        assert_eq!(actual, expected);

        let contents = "x = 'abc'";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = LogicalLines::from_tokens(&lxr, &locator)
            .into_iter()
            .map(|line| line.text_trimmed().to_string())
            .collect();
        let expected = vec!["x = 'abc'".to_string()];
        assert_eq!(actual, expected);

        let contents = r#"
def f():
  x = 1
f()"#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = LogicalLines::from_tokens(&lxr, &locator)
            .into_iter()
            .map(|line| line.text_trimmed().to_string())
            .collect();
        let expected = vec!["def f():", "x = 1", "f()"];
        assert_eq!(actual, expected);

        let contents = r#"
def f():
  """Docstring goes here."""
  # Comment goes here.
  x = 1
f()"#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = LogicalLines::from_tokens(&lxr, &locator)
            .into_iter()
            .map(|line| line.text_trimmed().to_string())
            .collect();
        let expected = vec![
            "def f():",
            "\"\"\"Docstring goes here.\"\"\"",
            "",
            "x = 1",
            "f()",
        ];
        assert_eq!(actual, expected);
    }
}
