use bisection::bisect_left;
use itertools::Itertools;
use rustpython_ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::rules::space_around_operator;
use crate::settings::Settings;
use crate::source_code::Locator;

#[derive(Debug)]
struct LogicalLine {
    text: String,
    mapping: Vec<(usize, Location)>,
    /// Whether the logical line contains an operator.
    operator: bool,
}

fn build_line(tokens: &[(Location, &Tok, Location)], locator: &Locator) -> LogicalLine {
    let mut logical = String::with_capacity(88);
    let mut operator = false;
    let mut mapping = Vec::new();
    let mut prev: Option<&Location> = None;
    let mut length = 0;
    for (start, tok, end) in tokens {
        if matches!(
            tok,
            Tok::Newline | Tok::NonLogicalNewline | Tok::Indent | Tok::Dedent | Tok::Comment { .. }
        ) {
            continue;
        }
        if mapping.is_empty() {
            mapping.push((0, *start));
        }

        if !operator {
            operator |= matches!(
                tok,
                Tok::Amper
                    | Tok::AmperEqual
                    | Tok::CircumFlex
                    | Tok::CircumflexEqual
                    | Tok::Colon
                    | Tok::ColonEqual
                    | Tok::DoubleSlash
                    | Tok::DoubleSlashEqual
                    | Tok::DoubleStar
                    | Tok::Equal
                    | Tok::Greater
                    | Tok::GreaterEqual
                    | Tok::Less
                    | Tok::LessEqual
                    | Tok::Minus
                    | Tok::MinusEqual
                    | Tok::NotEqual
                    | Tok::Percent
                    | Tok::PercentEqual
                    | Tok::Plus
                    | Tok::PlusEqual
                    | Tok::Slash
                    | Tok::SlashEqual
                    | Tok::Star
                    | Tok::StarEqual
                    | Tok::Vbar
                    | Tok::VbarEqual
            );
        }

        // TODO(charlie): "Mute" strings.
        let text = if let Tok::String { .. } = tok {
            "\"\""
        } else {
            locator.slice_source_code_range(&Range {
                location: *start,
                end_location: *end,
            })
        };

        if let Some(prev) = prev {
            if prev.row() != start.row() {
                let prev_text = locator.slice_source_code_range(&Range {
                    location: *prev,
                    end_location: Location::new(prev.row() + 1, 0),
                });
                if prev_text == ","
                    || ((prev_text != "{" && prev_text != "[" && prev_text != "(")
                        && (text != "}" || text != "]" || text != ")"))
                {
                    logical.push(' ');
                    length += 1;
                }
            } else if prev.column() != start.column() {
                let prev_text = locator.slice_source_code_range(&Range {
                    location: *prev,
                    end_location: *start,
                });
                logical.push_str(prev_text);
                length += prev_text.len();
            }
        }
        logical.push_str(text);
        length += text.len();
        mapping.push((length, *end));
        prev = Some(end);
    }

    LogicalLine {
        text: logical,
        operator,
        mapping,
    }
}

fn iter_logical_lines(tokens: &[LexResult], locator: &Locator) -> Vec<LogicalLine> {
    let mut parens = 0;
    let mut accumulator = Vec::with_capacity(32);
    let mut lines = Vec::with_capacity(128);
    for &(start, ref tok, end) in tokens.iter().flatten() {
        accumulator.push((start, tok, end));
        if matches!(tok, Tok::Lbrace | Tok::Lpar | Tok::Lsqb) {
            parens += 1;
        } else if matches!(tok, Tok::Rbrace | Tok::Rpar | Tok::Rsqb) {
            parens -= 1;
        } else if parens == 0 {
            if matches!(tok, Tok::Newline) {
                lines.push(build_line(&accumulator, locator));
                accumulator.drain(..);
            }
        }
    }
    lines
}

pub fn check_logical_lines(
    tokens: &[LexResult],
    locator: &Locator,
    settings: &Settings,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    for line in iter_logical_lines(tokens, locator) {
        if line.operator {
            let mapping_offsets = line.mapping.iter().map(|(offset, _)| *offset).collect_vec();
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
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use rustpython_parser::lexer;
    use rustpython_parser::lexer::LexResult;

    use crate::checkers::logical_lines::iter_logical_lines;
    use crate::source_code::Locator;

    #[test]
    fn split_logical_lines() {
        let contents = r#"
x = 1
y = 2
z = x + 1"#;
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
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
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = iter_logical_lines(&lxr, &locator)
            .into_iter()
            .map(|line| line.text)
            .collect();
        let expected = vec![
            "x = [ 1, 2, 3, ]".to_string(),
            "y = 2".to_string(),
            "z = x + 1".to_string(),
        ];
        assert_eq!(actual, expected);

        let contents = "x = 'abc'";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = iter_logical_lines(&lxr, &locator)
            .into_iter()
            .map(|line| line.text)
            .collect();
        let expected = vec!["x = \"\"".to_string()];
        assert_eq!(actual, expected);

        let contents = r#"
def f():
  x = 1
f()"#;
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
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
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = iter_logical_lines(&lxr, &locator)
            .into_iter()
            .map(|line| line.text)
            .collect();
        let expected = vec!["def f():", "\"\"", "x = 1", "f()"];
        assert_eq!(actual, expected);
    }
}
