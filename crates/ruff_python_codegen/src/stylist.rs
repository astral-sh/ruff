//! Detect code style from Python source code.

use std::fmt;
use std::ops::Deref;

use once_cell::unsync::OnceCell;
use ruff_python_literal::escape::Quote as StrQuote;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_source_file::{find_newline, LineEnding};

use ruff_python_ast::str::leading_quote;
use ruff_source_file::Locator;

pub struct Stylist<'a> {
    locator: &'a Locator<'a>,
    indentation: Indentation,
    quote: Quote,
    line_ending: OnceCell<LineEnding>,
}

impl<'a> Stylist<'a> {
    pub fn indentation(&'a self) -> &'a Indentation {
        &self.indentation
    }

    pub fn quote(&'a self) -> Quote {
        self.quote
    }

    pub fn line_ending(&'a self) -> LineEnding {
        *self.line_ending.get_or_init(|| {
            let contents = self.locator.contents();
            find_newline(contents)
                .map(|(_, ending)| ending)
                .unwrap_or_default()
        })
    }

    pub fn from_tokens(tokens: &[LexResult], locator: &'a Locator<'a>) -> Self {
        let indentation = detect_indention(tokens, locator);

        Self {
            locator,
            indentation,
            quote: detect_quote(tokens, locator),
            line_ending: OnceCell::default(),
        }
    }
}

fn detect_quote(tokens: &[LexResult], locator: &Locator) -> Quote {
    let quote_range = tokens.iter().flatten().find_map(|(t, range)| match t {
        Tok::String {
            triple_quoted: false,
            ..
        } => Some(*range),
        // No need to check if it's triple-quoted as f-strings cannot be used
        // as docstrings.
        Tok::FStringStart => Some(*range),
        _ => None,
    });

    if let Some(quote_range) = quote_range {
        let content = &locator.slice(quote_range);
        if let Some(quotes) = leading_quote(content) {
            return if quotes.contains('\'') {
                Quote::Single
            } else if quotes.contains('"') {
                Quote::Double
            } else {
                unreachable!("Expected string to start with a valid quote prefix")
            };
        }
    }

    Quote::default()
}

fn detect_indention(tokens: &[LexResult], locator: &Locator) -> Indentation {
    let indent_range = tokens.iter().flatten().find_map(|(t, range)| {
        if matches!(t, Tok::Indent) {
            Some(range)
        } else {
            None
        }
    });

    if let Some(indent_range) = indent_range {
        let mut whitespace = locator.slice(*indent_range);
        // https://docs.python.org/3/reference/lexical_analysis.html#indentation
        // > A formfeed character may be present at the start of the line; it will be ignored for
        // > the indentation calculations above. Formfeed characters occurring elsewhere in the
        // > leading whitespace have an undefined effect (for instance, they may reset the space
        // > count to zero).
        // So there's UB in python lexer -.-
        // In practice, they just reset the indentation:
        // https://github.com/python/cpython/blob/df8b3a46a7aa369f246a09ffd11ceedf1d34e921/Parser/tokenizer.c#L1819-L1821
        // https://github.com/astral-sh/ruff/blob/a41bb2733fe75a71f4cf6d4bb21e659fc4630b30/crates/ruff_python_parser/src/lexer.rs#L664-L667
        // We also reset the indentation when we see a formfeed character.
        // See also https://github.com/astral-sh/ruff/issues/7455#issuecomment-1722458825
        if let Some((_before, after)) = whitespace.rsplit_once('\x0C') {
            whitespace = after;
        }

        Indentation(whitespace.to_string())
    } else {
        Indentation::default()
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

impl From<Quote> for StrQuote {
    fn from(val: Quote) -> Self {
        match val {
            Quote::Single => StrQuote::Single,
            Quote::Double => StrQuote::Double,
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

#[cfg(test)]
mod tests {
    use ruff_python_parser::lexer::lex;
    use ruff_python_parser::Mode;

    use ruff_source_file::{find_newline, LineEnding};

    use super::{Indentation, Quote, Stylist};
    use ruff_source_file::Locator;

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

        // formfeed indent, see `detect_indention` comment.
        let contents = r#"
class FormFeedIndent:
   def __init__(self, a=[]):
        print(a)
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).indentation(),
            &Indentation(" ".to_string())
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

        let contents = r#"x = f'1'"#;
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

        let contents = r#"x = f"1""#;
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

        // Detect from f-string appearing after docstring
        let contents = r#"
"""Module docstring."""

a = f'v'
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::Single
        );

        let contents = r#"
'''Module docstring.'''

a = f"v"
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::Double
        );

        let contents = r#"
f'''Module docstring.'''
"#;
        let locator = Locator::new(contents);
        let tokens: Vec<_> = lex(contents, Mode::Module).collect();
        assert_eq!(
            Stylist::from_tokens(&tokens, &locator).quote(),
            Quote::Single
        );
    }

    #[test]
    fn line_ending() {
        let contents = "x = 1";
        assert_eq!(find_newline(contents).map(|(_, ending)| ending), None);

        let contents = "x = 1\n";
        assert_eq!(
            find_newline(contents).map(|(_, ending)| ending),
            Some(LineEnding::Lf)
        );

        let contents = "x = 1\r";
        assert_eq!(
            find_newline(contents).map(|(_, ending)| ending),
            Some(LineEnding::Cr)
        );

        let contents = "x = 1\r\n";
        assert_eq!(
            find_newline(contents).map(|(_, ending)| ending),
            Some(LineEnding::CrLf)
        );
    }
}
