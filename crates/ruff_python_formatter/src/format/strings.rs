use rustpython_parser::{Mode, Tok};

use crate::prelude::*;
use ruff_formatter::{write, Format};
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_text_size::TextRange;

use crate::cst::Expr;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct StringLiteralPart {
    range: TextRange,
}

impl Format<ASTFormatContext<'_>> for StringLiteralPart {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let contents = f.context().contents();

        // Extract leading and trailing quotes.
        let contents = &contents[self.range];
        let leading_quote = leading_quote(contents).unwrap();
        let trailing_quote = trailing_quote(contents).unwrap();
        let body = &contents[leading_quote.len()..contents.len() - trailing_quote.len()];

        // Determine the correct quote style.
        // TODO(charlie): Make this parameterizable.
        let mut squotes: usize = 0;
        let mut dquotes: usize = 0;
        for char in body.chars() {
            if char == '\'' {
                squotes += 1;
            } else if char == '"' {
                dquotes += 1;
            }
        }

        // Retain raw prefixes.
        let mut is_raw = false;
        if leading_quote.contains('r') {
            is_raw = true;
            f.write_element(FormatElement::StaticText { text: "r" })?;
        } else if leading_quote.contains('R') {
            is_raw = true;
            f.write_element(FormatElement::StaticText { text: "R" })?;
        }

        // Normalize bytes literals to use b"...".
        if leading_quote.contains('b') || leading_quote.contains('B') {
            f.write_element(FormatElement::StaticText { text: "b" })?;
        }

        // TODO(charlie): Avoid allocating if there's nothing to escape. In other words, if we can
        // use the string body directly, do so!
        if trailing_quote.len() == 1 {
            // Single-quoted string.
            if dquotes == 0 || squotes > 0 {
                // If the body doesn't contain any double quotes, or it contains both single and
                // double quotes, use double quotes.
                f.write_element(FormatElement::StaticText { text: "\"" })?;
                f.write_element(FormatElement::DynamicText {
                    text: if is_raw {
                        body.into()
                    } else {
                        double_escape(body).into()
                    },
                })?;
                f.write_element(FormatElement::StaticText { text: "\"" })?;
                Ok(())
            } else {
                f.write_element(FormatElement::StaticText { text: "'" })?;
                f.write_element(FormatElement::DynamicText {
                    text: if is_raw {
                        body.into()
                    } else {
                        single_escape(body).into()
                    },
                })?;
                f.write_element(FormatElement::StaticText { text: "'" })?;
                Ok(())
            }
        } else if trailing_quote.len() == 3 {
            // Triple-quoted string.
            if body.starts_with("\"\"\"") || body.ends_with('"') {
                // We only need to use single quotes if the string body starts with three or more
                // double quotes, or ends with a double quote. Converting to double quotes in those
                // cases would cause a syntax error.
                f.write_element(FormatElement::StaticText { text: "'''" })?;
                f.write_element(FormatElement::DynamicText {
                    text: body.to_string().into_boxed_str(),
                })?;
                f.write_element(FormatElement::StaticText { text: "'''" })?;
                Ok(())
            } else {
                f.write_element(FormatElement::StaticText { text: "\"\"\"" })?;
                f.write_element(FormatElement::DynamicText {
                    text: body.to_string().into_boxed_str(),
                })?;
                f.write_element(FormatElement::StaticText { text: "\"\"\"" })?;
                Ok(())
            }
        } else {
            unreachable!("Invalid quote length: {}", trailing_quote.len());
        }
    }
}

#[inline]
pub(crate) const fn string_literal_part(range: TextRange) -> StringLiteralPart {
    StringLiteralPart { range }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct StringLiteral<'a> {
    expr: &'a Expr,
}

impl Format<ASTFormatContext<'_>> for StringLiteral<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let expr = self.expr;

        // TODO(charlie): This tokenization needs to happen earlier, so that we can attach
        // comments to individual string literals.
        let contents = f.context().locator().slice(expr.range());
        let elts = rustpython_parser::lexer::lex_starts_at(contents, Mode::Module, expr.start())
            .flatten()
            .filter_map(|(tok, range)| {
                if matches!(tok, Tok::String { .. }) {
                    Some(range)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        write!(
            f,
            [group(&format_with(|f| {
                if expr.parentheses.is_if_expanded() {
                    write!(f, [if_group_breaks(&text("("))])?;
                }
                for (i, elt) in elts.iter().enumerate() {
                    write!(f, [string_literal_part(*elt)])?;
                    if i < elts.len() - 1 {
                        write!(f, [soft_line_break_or_space()])?;
                    }
                }
                if expr.parentheses.is_if_expanded() {
                    write!(f, [if_group_breaks(&text(")"))])?;
                }
                Ok(())
            }))]
        )?;
        Ok(())
    }
}

#[inline]
pub(crate) const fn string_literal(expr: &Expr) -> StringLiteral {
    StringLiteral { expr }
}

/// Escape a string body to be used in a string literal with double quotes.
fn double_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let Some(next) = chars.peek() else {
                break;
            };
            if *next == '\'' {
                chars.next();
                escaped.push('\'');
            } else if *next == '"' {
                chars.next();
                escaped.push('\\');
                escaped.push('"');
            } else if *next == '\\' {
                chars.next();
                escaped.push('\\');
                escaped.push(ch);
            } else {
                escaped.push(ch);
            }
        } else if ch == '"' {
            escaped.push('\\');
            escaped.push('"');
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

/// Escape a string body to be used in a string literal with single quotes.
fn single_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let Some(next) = chars.peek() else {
                break;
            };
            if *next == '"' {
                chars.next();
                escaped.push('"');
            } else if *next == '\'' {
                chars.next();
                escaped.push('\\');
                escaped.push('\'');
            } else if *next == '\\' {
                chars.next();
                escaped.push('\\');
                escaped.push(ch);
            } else {
                escaped.push(ch);
            }
        } else if ch == '\'' {
            escaped.push('\\');
            escaped.push('\'');
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_escape() {
        assert_eq!(double_escape(r#"It\'s mine"#), r#"It's mine"#);
        assert_eq!(double_escape(r#"It\'s "mine""#), r#"It's \"mine\""#);
        assert_eq!(double_escape(r#"It\\'s mine"#), r#"It\\'s mine"#);
    }

    #[test]
    fn test_single_escape() {
        assert_eq!(single_escape(r#"It's mine"#), r#"It\'s mine"#);
        assert_eq!(single_escape(r#"It\'s "mine""#), r#"It\'s "mine""#);
        assert_eq!(single_escape(r#"It\\'s mine"#), r#"It\\\'s mine"#);
    }
}
