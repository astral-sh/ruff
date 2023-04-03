//! Detect code style from Python source code.

use std::fmt;
use std::ops::Deref;

use once_cell::unsync::OnceCell;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_rustpython::vendor;

use crate::source_code::Locator;
use crate::str::leading_quote;
use crate::types::Range;

pub struct Stylist<'a> {
    locator: &'a Locator<'a>,
    indentation: OnceCell<Indentation>,
    indent_end: Option<Location>,
    quote: OnceCell<Quote>,
    quote_range: Option<Range>,
    line_ending: OnceCell<LineEnding>,
}

impl<'a> Stylist<'a> {
    pub fn indentation(&'a self) -> &'a Indentation {
        self.indentation.get_or_init(|| {
            if let Some(indent_end) = self.indent_end {
                let start = Location::new(indent_end.row(), 0);
                let whitespace = self.locator.slice(Range::new(start, indent_end));
                Indentation(whitespace.to_string())
            } else {
                Indentation::default()
            }
        })
    }

    pub fn quote(&'a self) -> Quote {
        *self.quote.get_or_init(|| {
            self.quote_range
                .and_then(|quote_range| {
                    let content = self.locator.slice(quote_range);
                    leading_quote(content)
                })
                .map(|pattern| {
                    if pattern.contains('\'') {
                        Quote::Single
                    } else if pattern.contains('"') {
                        Quote::Double
                    } else {
                        unreachable!("Expected string to start with a valid quote prefix")
                    }
                })
                .unwrap_or_default()
        })
    }

    pub fn line_ending(&'a self) -> LineEnding {
        *self
            .line_ending
            .get_or_init(|| detect_line_ending(self.locator.contents()).unwrap_or_default())
    }

    pub fn from_tokens(tokens: &[LexResult], locator: &'a Locator<'a>) -> Self {
        let indent_end = tokens.iter().flatten().find_map(|(_, t, end)| {
            if matches!(t, Tok::Indent) {
                Some(*end)
            } else {
                None
            }
        });

        let quote_range = tokens.iter().flatten().find_map(|(start, t, end)| match t {
            Tok::String {
                triple_quoted: false,
                ..
            } => Some(Range::new(*start, *end)),
            _ => None,
        });

        Self {
            locator,
            indentation: OnceCell::default(),
            indent_end,
            quote_range,
            quote: OnceCell::default(),
            line_ending: OnceCell::default(),
        }
    }
}

/// The quotation style used in Python source code.
#[derive(Debug, Default, PartialEq, Eq, Copy, Clone)]
pub enum Quote {
    Single,
    #[default]
    Double,
}

impl From<Quote> for char {
    fn from(val: Quote) -> Self {
        match val {
            Quote::Single => '\'',
            Quote::Double => '"',
        }
    }
}

impl From<Quote> for vendor::str::Quote {
    fn from(val: Quote) -> Self {
        match val {
            Quote::Single => vendor::str::Quote::Single,
            Quote::Double => vendor::str::Quote::Double,
        }
    }
}

impl fmt::Display for Quote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Quote::Single => write!(f, "\'"),
            Quote::Double => write!(f, "\""),
        }
    }
}

/// The indentation style used in Python source code.
#[derive(Debug, PartialEq, Eq)]
pub struct Indentation(String);

impl Indentation {
    pub const fn new(indentation: String) -> Self {
        Self(indentation)
    }
}

impl Default for Indentation {
    fn default() -> Self {
        Indentation("    ".to_string())
    }
}

impl Indentation {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn as_char(&self) -> char {
        self.0.chars().next().unwrap()
    }
}

impl Deref for Indentation {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

/// The line ending style used in Python source code.
/// See <https://docs.python.org/3/reference/lexical_analysis.html#physical-lines>
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum LineEnding {
    Lf,
    Cr,
    CrLf,
}

impl Default for LineEnding {
    fn default() -> Self {
        if cfg!(windows) {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        }
    }
}

impl LineEnding {
    pub const fn as_str(&self) -> &'static str {
        match self {
            LineEnding::CrLf => "\r\n",
            LineEnding::Lf => "\n",
            LineEnding::Cr => "\r",
        }
    }
}

impl Deref for LineEnding {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

/// Detect the line ending style of the given contents.
fn detect_line_ending(contents: &str) -> Option<LineEnding> {
    if let Some(position) = contents.find('\n') {
        let position = position.saturating_sub(1);
        return if let Some('\r') = contents.chars().nth(position) {
            Some(LineEnding::CrLf)
        } else {
            Some(LineEnding::Lf)
        };
    } else if contents.find('\r').is_some() {
        return Some(LineEnding::Cr);
    }
    None
}

#[cfg(test)]
mod tests {
    use rustpython_parser::lexer::lex;
    use rustpython_parser::Mode;

    use crate::source_code::stylist::{detect_line_ending, Indentation, LineEnding, Quote};
    use crate::source_code::{Locator, Stylist};

    #[test]
    fn indentation() {
        let contents = r#"x = 1"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).indentation(),
            &Indentation::default()
        );

        let contents = r#"
if True:
  pass
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).indentation(),
            &Indentation("  ".to_string())
        );

        let contents = r#"
if True:
    pass
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).indentation(),
            &Indentation("    ".to_string())
        );

        let contents = r#"
if True:
	pass
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).indentation(),
            &Indentation("\t".to_string())
        );

        // TODO(charlie): Should non-significant whitespace be detected?
        let contents = r#"
x = (
  1,
  2,
  3,
)
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).indentation(),
            &Indentation::default()
        );
    }

    #[test]
    fn quote() {
        let contents = r#"x = 1"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::default()
        );

        let contents = r#"x = '1'"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::Single
        );

        let contents = r#"x = "1""#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::Double
        );

        let contents = r#"s = "It's done.""#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::Double
        );

        // No style if only double quoted docstring (will take default Double)
        let contents = r#"
def f():
    """Docstring."""
    pass
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::default()
        );

        // Detect from string literal appearing after docstring
        let contents = r#"
"""Module docstring."""

a = 'v'
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::Single
        );

        let contents = r#"
'''Module docstring.'''

a = "v"
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::Double
        );
    }

    #[test]
    fn line_ending() {
        let contents = "x = 1";
        assert_eq!(detect_line_ending(contents), None);

        let contents = "x = 1\n";
        assert_eq!(detect_line_ending(contents), Some(LineEnding::Lf));

        let contents = "x = 1\r";
        assert_eq!(detect_line_ending(contents), Some(LineEnding::Cr));

        let contents = "x = 1\r\n";
        assert_eq!(detect_line_ending(contents), Some(LineEnding::CrLf));
    }
}
