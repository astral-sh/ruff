//! Detect code style from Python source code.

use std::fmt;
use std::ops::Deref;

use once_cell::unsync::OnceCell;
use rustpython_ast::Location;
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::pydocstyle::helpers::leading_quote;
use crate::source_code_locator::SourceCodeLocator;
use crate::vendor;

pub struct SourceCodeStyleDetector<'a> {
    contents: &'a str,
    locator: &'a SourceCodeLocator<'a>,
    indentation: OnceCell<Indentation>,
    quote: OnceCell<Quote>,
    line_ending: OnceCell<LineEnding>,
}

impl<'a> SourceCodeStyleDetector<'a> {
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

    pub fn from_contents(contents: &'a str, locator: &'a SourceCodeLocator<'a>) -> Self {
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
#[derive(Debug, PartialEq, Eq)]
pub enum Quote {
    Single,
    Double,
}

impl Default for Quote {
    fn default() -> Self {
        Quote::Double
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
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
    pub fn new(indentation: String) -> Self {
        Self(indentation)
    }
}

impl Default for Indentation {
    fn default() -> Self {
        Indentation("    ".to_string())
    }
}

impl Deref for Indentation {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
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
        LineEnding::Lf
    }
}

impl Deref for LineEnding {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match &self {
            LineEnding::CrLf => "\r\n",
            LineEnding::Lf => "\n",
            LineEnding::Cr => "\r",
        }
    }
}

/// Detect the indentation style of the given tokens.
fn detect_indentation(contents: &str, locator: &SourceCodeLocator) -> Option<Indentation> {
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
fn detect_quote(contents: &str, locator: &SourceCodeLocator) -> Option<Quote> {
    for (start, tok, end) in lexer::make_tokenizer(contents).flatten() {
        if let Tok::String { .. } = tok {
            let content = locator.slice_source_code_range(&Range::new(start, end));
            if let Some(pattern) = leading_quote(&content) {
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
    return if let Some(position) = contents.find('\r') {
        if let Some('\n') = contents.chars().nth(position + 1) {
            Some(LineEnding::CrLf)
        } else {
            Some(LineEnding::Cr)
        }
    } else if contents.find('\n').is_some() {
        Some(LineEnding::Lf)
    } else {
        None
    };
}

#[cfg(test)]
mod tests {
    use crate::source_code_style::{
        detect_indentation, detect_line_ending, detect_quote, Indentation, LineEnding, Quote,
    };
    use crate::SourceCodeLocator;

    #[test]
    fn indentation() {
        let contents = r#"x = 1"#;
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(detect_indentation(contents, &locator), None);

        let contents = r#"
if True:
  pass
"#;
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            detect_indentation(contents, &locator),
            Some(Indentation("  ".to_string()))
        );

        let contents = r#"
if True:
    pass
"#;
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            detect_indentation(contents, &locator),
            Some(Indentation("    ".to_string()))
        );

        let contents = r#"
if True:
	pass
"#;
        let locator = SourceCodeLocator::new(contents);
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
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(detect_indentation(contents, &locator), None);
    }

    #[test]
    fn quote() {
        let contents = r#"x = 1"#;
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(detect_quote(contents, &locator), None);

        let contents = r#"x = '1'"#;
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(detect_quote(contents, &locator), Some(Quote::Single));

        let contents = r#"x = "1""#;
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(detect_quote(contents, &locator), Some(Quote::Double));

        let contents = r#"
def f():
    """Docstring."""
    pass
"#;
        let locator = SourceCodeLocator::new(contents);
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

        let contents = "x = 1\ry = 2\n";
        assert_eq!(detect_line_ending(contents), Some(LineEnding::Cr));

        let contents = "x = 1\r\n";
        assert_eq!(detect_line_ending(contents), Some(LineEnding::CrLf));
    }
}
