//! Detect code style from Python source code.

use std::fmt;
use std::ops::Deref;

use once_cell::unsync::OnceCell;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::rules::pydocstyle::helpers::leading_quote;
use crate::source_code::Locator;
use crate::vendor;

pub struct Stylist<'a> {
    contents: &'a str,
    locator: &'a Locator<'a>,
    indentation: OnceCell<Indentation>,
    quote: OnceCell<Quote>,
    line_ending: OnceCell<LineEnding>,
}

impl<'a> Stylist<'a> {
    pub fn indentation(&'a self) -> &'a Indentation {
        self.indentation
            .get_or_init(|| detect_indentation(self.contents, self.locator).unwrap_or_default())
    }

    pub fn quote(&'a self) -> &'a Quote {
        self.quote
            .get_or_init(|| detect_quote(self.contents, self.locator).unwrap_or_default())
    }

    pub fn line_ending(&'a self) -> &'a LineEnding {
        self.line_ending
            .get_or_init(|| detect_line_ending(self.contents).unwrap_or_default())
    }

    pub fn from_contents(contents: &'a str, locator: &'a Locator<'a>) -> Self {
        Self {
            contents,
            locator,
            indentation: OnceCell::default(),
            quote: OnceCell::default(),
            line_ending: OnceCell::default(),
        }
    }
}

/// The quotation style used in Python source code.
#[derive(Debug, Default, PartialEq, Eq)]
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

impl From<&Quote> for vendor::str::Quote {
    fn from(val: &Quote) -> Self {
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

impl From<&Quote> for char {
    fn from(val: &Quote) -> Self {
        match val {
            Quote::Single => '\'',
            Quote::Double => '"',
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
#[derive(Debug, PartialEq, Eq)]
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

/// Detect the indentation style of the given tokens.
fn detect_indentation(contents: &str, locator: &Locator) -> Option<Indentation> {
    for (_start, tok, end) in lexer::make_tokenizer(contents).flatten() {
        if let Tok::Indent { .. } = tok {
            let start = Location::new(end.row(), 0);
            let whitespace = locator.slice_source_code_range(&Range::new(start, end));
            return Some(Indentation(whitespace.to_string()));
        }
    }
    None
}

/// Detect the quotation style of the given tokens.
fn detect_quote(contents: &str, locator: &Locator) -> Option<Quote> {
    for (start, tok, end) in lexer::make_tokenizer(contents).flatten() {
        if let Tok::String { .. } = tok {
            let content = locator.slice_source_code_range(&Range::new(start, end));
            if let Some(pattern) = leading_quote(content) {
                if pattern.contains('\'') {
                    return Some(Quote::Single);
                } else if pattern.contains('"') {
                    return Some(Quote::Double);
                }
                unreachable!("Expected string to start with a valid quote prefix")
            }
        }
    }
    None
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
    use crate::source_code::stylist::{
        detect_indentation, detect_line_ending, detect_quote, Indentation, LineEnding, Quote,
    };
    use crate::source_code::Locator;

    #[test]
    fn indentation() {
        let contents = r#"x = 1"#;
        let locator = Locator::new(contents);
        assert_eq!(detect_indentation(contents, &locator), None);

        let contents = r#"
if True:
  pass
"#;
        let locator = Locator::new(contents);
        assert_eq!(
            detect_indentation(contents, &locator),
            Some(Indentation("  ".to_string()))
        );

        let contents = r#"
if True:
    pass
"#;
        let locator = Locator::new(contents);
        assert_eq!(
            detect_indentation(contents, &locator),
            Some(Indentation("    ".to_string()))
        );

        let contents = r#"
if True:
	pass
"#;
        let locator = Locator::new(contents);
        assert_eq!(
            detect_indentation(contents, &locator),
            Some(Indentation("\t".to_string()))
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
        assert_eq!(detect_indentation(contents, &locator), None);
    }

    #[test]
    fn quote() {
        let contents = r#"x = 1"#;
        let locator = Locator::new(contents);
        assert_eq!(detect_quote(contents, &locator), None);

        let contents = r#"x = '1'"#;
        let locator = Locator::new(contents);
        assert_eq!(detect_quote(contents, &locator), Some(Quote::Single));

        let contents = r#"x = "1""#;
        let locator = Locator::new(contents);
        assert_eq!(detect_quote(contents, &locator), Some(Quote::Double));

        let contents = r#"
def f():
    """Docstring."""
    pass
"#;
        let locator = Locator::new(contents);
        assert_eq!(detect_quote(contents, &locator), Some(Quote::Double));
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
