use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;

use ruff_diagnostics::{Diagnostic, Fix};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::types::Range;

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
    let mut diagnostics = vec![];

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
            for (location, kind) in space_around_operator(&line) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: Fix::empty(),
                        parent: None,
                    });
                }
            }

            for (location, kind) in whitespace_around_named_parameter_equals(&line.tokens()) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: Fix::empty(),
                        parent: None,
                    });
                }
            }
            for (location, kind) in missing_whitespace_around_operator(&line.tokens()) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: Fix::empty(),
                        parent: None,
                    });
                }
            }

            for diagnostic in missing_whitespace(&line, should_fix_missing_whitespace) {
                if settings.rules.enabled(diagnostic.kind.rule()) {
                    diagnostics.push(diagnostic);
                }
            }
        }
        if line
            .flags()
            .contains(TokenFlags::OPERATOR | TokenFlags::PUNCTUATION)
        {
            for (location, kind) in extraneous_whitespace(&line) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: Fix::empty(),
                        parent: None,
                    });
                }
            }
        }
        if line.flags().contains(TokenFlags::KEYWORD) {
            for (location, kind) in whitespace_around_keywords(&line) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: Fix::empty(),
                        parent: None,
                    });
                }
            }

            for (location, kind) in missing_whitespace_after_keyword(&line.tokens()) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: Fix::empty(),
                        parent: None,
                    });
                }
            }
        }
        if line.flags().contains(TokenFlags::COMMENT) {
            for (range, kind) in whitespace_before_comment(&line.tokens(), locator) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location: range.location,
                        end_location: range.end_location,
                        fix: Fix::empty(),
                        parent: None,
                    });
                }
            }
        }

        if line.flags().contains(TokenFlags::BRACKET) {
            for diagnostic in whitespace_before_parameters(
                &line.tokens(),
                should_fix_whitespace_before_parameters,
            ) {
                if settings.rules.enabled(diagnostic.kind.rule()) {
                    diagnostics.push(diagnostic);
                }
            }
        }

        // Extract the indentation level.
        let Some(start_loc) = line.first_token_location() else { continue; };
        let start_line = locator.slice(Range::new(Location::new(start_loc.row(), 0), start_loc));
        let indent_level = expand_indent(start_line);
        let indent_size = 4;

        for (location, kind) in indentation(
            &line,
            prev_line.as_ref(),
            indent_char,
            indent_level,
            prev_indent_level,
            indent_size,
        ) {
            if settings.rules.enabled(kind.rule()) {
                diagnostics.push(Diagnostic {
                    kind,
                    location: Location::new(start_loc.row(), 0),
                    end_location: location,
                    fix: Fix::empty(),
                    parent: None,
                });
            }
        }

        if !line.is_comment_only() {
            prev_line = Some(line);
            prev_indent_level = Some(indent_level);
        }
    }
    diagnostics
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
