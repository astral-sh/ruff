#![allow(dead_code, unused_imports, unused_variables)]

use bisection::bisect_left;
use itertools::Itertools;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::types::Range;

use crate::registry::{AsRule, Rule};
use crate::rules::pycodestyle::logical_lines::{iter_logical_lines, TokenFlags};
use crate::rules::pycodestyle::rules::{
    extraneous_whitespace, indentation, missing_whitespace, missing_whitespace_after_keyword,
    missing_whitespace_around_operator, space_around_operator, whitespace_around_keywords,
    whitespace_around_named_parameter_equals, whitespace_before_comment,
    whitespace_before_parameters,
};
use crate::settings::{flags, Settings};

/// Return the amount of indentation, expanding tabs to the next multiple of 8.
fn expand_indent(mut line: &str) -> usize {
    while line.ends_with("\n\r") {
        line = &line[..line.len() - 2];
    }
    if !line.contains('\t') {
        return line.len() - line.trim_start().len();
    }
    let mut indent = 0;
    for c in line.chars() {
        if c == '\t' {
            indent = (indent / 8) * 8 + 8;
        } else if c == ' ' {
            indent += 1;
        } else {
            break;
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

    let indent_char = stylist.indentation().as_char();
    let mut prev_line = None;
    let mut prev_indent_level = None;
    for line in iter_logical_lines(tokens, locator) {
        if line.mapping.is_empty() {
            continue;
        }

        // Extract the indentation level.
        let start_loc = line.mapping[0].1;
        let start_line = locator.slice(Range::new(Location::new(start_loc.row(), 0), start_loc));
        let indent_level = expand_indent(start_line);
        let indent_size = 4;

        // Generate mapping from logical to physical offsets.
        let mapping_offsets = line.mapping.iter().map(|(offset, _)| *offset).collect_vec();

        if line.flags.contains(TokenFlags::OPERATOR) {
            for (index, kind) in space_around_operator(&line.text) {
                let (token_offset, pos) = line.mapping[bisect_left(&mapping_offsets, &index)];
                let location = Location::new(pos.row(), pos.column() + index - token_offset);
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: None,
                        parent: None,
                    });
                }
            }
        }
        if line
            .flags
            .contains(TokenFlags::OPERATOR | TokenFlags::PUNCTUATION)
        {
            for (index, kind) in extraneous_whitespace(&line.text) {
                let (token_offset, pos) = line.mapping[bisect_left(&mapping_offsets, &index)];
                let location = Location::new(pos.row(), pos.column() + index - token_offset);
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: None,
                        parent: None,
                    });
                }
            }
        }
        if line.flags.contains(TokenFlags::KEYWORD) {
            for (index, kind) in whitespace_around_keywords(&line.text) {
                let (token_offset, pos) = line.mapping[bisect_left(&mapping_offsets, &index)];
                let location = Location::new(pos.row(), pos.column() + index - token_offset);
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: None,
                        parent: None,
                    });
                }
            }

            for (location, kind) in missing_whitespace_after_keyword(&line.tokens) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: None,
                        parent: None,
                    });
                }
            }
        }
        if line.flags.contains(TokenFlags::COMMENT) {
            for (range, kind) in whitespace_before_comment(&line.tokens, locator) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location: range.location,
                        end_location: range.end_location,
                        fix: None,
                        parent: None,
                    });
                }
            }
        }
        if line.flags.contains(TokenFlags::OPERATOR) {
            for (location, kind) in
                whitespace_around_named_parameter_equals(&line.tokens, &line.text)
            {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: None,
                        parent: None,
                    });
                }
            }
            for (location, kind) in missing_whitespace_around_operator(&line.tokens) {
                if settings.rules.enabled(kind.rule()) {
                    diagnostics.push(Diagnostic {
                        kind,
                        location,
                        end_location: location,
                        fix: None,
                        parent: None,
                    });
                }
            }

            #[cfg(debug_assertions)]
            let should_fix = autofix.into() && settings.rules.should_fix(Rule::MissingWhitespace);

            #[cfg(not(debug_assertions))]
            let should_fix = false;

            for diagnostic in
                missing_whitespace(&line.text, start_loc.row(), should_fix, indent_level)
            {
                if settings.rules.enabled(diagnostic.kind.rule()) {
                    diagnostics.push(diagnostic);
                }
            }
        }

        if line.flags.contains(TokenFlags::BRACKET) {
            #[cfg(debug_assertions)]
            let should_fix =
                autofix.into() && settings.rules.should_fix(Rule::WhitespaceBeforeParameters);

            #[cfg(not(debug_assertions))]
            let should_fix = false;

            for diagnostic in whitespace_before_parameters(&line.tokens, should_fix) {
                if settings.rules.enabled(diagnostic.kind.rule()) {
                    diagnostics.push(diagnostic);
                }
            }
        }

        for (index, kind) in indentation(
            &line,
            prev_line.as_ref(),
            indent_char,
            indent_level,
            prev_indent_level,
            indent_size,
        ) {
            let (token_offset, pos) = line.mapping[bisect_left(&mapping_offsets, &index)];
            let location = Location::new(pos.row(), pos.column() + index - token_offset);
            if settings.rules.enabled(kind.rule()) {
                diagnostics.push(Diagnostic {
                    kind,
                    location,
                    end_location: location,
                    fix: None,
                    parent: None,
                });
            }
        }

        if !line.is_comment() {
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

    use ruff_python_ast::source_code::Locator;

    use crate::checkers::logical_lines::iter_logical_lines;

    #[test]
    fn split_logical_lines() {
        let contents = r#"
x = 1
y = 2
z = x + 1"#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = iter_logical_lines(&lxr, &locator)
            .into_iter()
            .map(|line| line.text)
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
        let actual: Vec<String> = iter_logical_lines(&lxr, &locator)
            .into_iter()
            .map(|line| line.text)
            .collect();
        let expected = vec![
            "x = [1, 2, 3, ]".to_string(),
            "y = 2".to_string(),
            "z = x + 1".to_string(),
        ];
        assert_eq!(actual, expected);

        let contents = "x = 'abc'";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = iter_logical_lines(&lxr, &locator)
            .into_iter()
            .map(|line| line.text)
            .collect();
        let expected = vec!["x = \"xxx\"".to_string()];
        assert_eq!(actual, expected);

        let contents = r#"
def f():
  x = 1
f()"#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = iter_logical_lines(&lxr, &locator)
            .into_iter()
            .map(|line| line.text)
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
        let actual: Vec<String> = iter_logical_lines(&lxr, &locator)
            .into_iter()
            .map(|line| line.text)
            .collect();
        let expected = vec!["def f():", "\"xxxxxxxxxxxxxxxxxxxx\"", "", "x = 1", "f()"];
        assert_eq!(actual, expected);
    }
}
