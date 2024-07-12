//! Detect code style from Python source code.

use std::ops::Deref;

use once_cell::unsync::OnceCell;

use ruff_python_ast::str::Quote;
use ruff_python_parser::{Token, TokenKind, Tokens};
use ruff_source_file::{find_newline, LineEnding, Locator};
use ruff_text_size::Ranged;

#[derive(Debug, Clone)]
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

    pub fn from_tokens(tokens: &Tokens, locator: &'a Locator<'a>) -> Self {
        let indentation = detect_indentation(tokens, locator);

        Self {
            locator,
            indentation,
            quote: detect_quote(tokens),
            line_ending: OnceCell::default(),
        }
    }
}

fn detect_quote(tokens: &[Token]) -> Quote {
    for token in tokens {
        match token.kind() {
            TokenKind::String if !token.is_triple_quoted_string() => {
                return token.string_quote_style()
            }
            TokenKind::FStringStart => return token.string_quote_style(),
            _ => continue,
        }
    }
    Quote::default()
}

fn detect_indentation(tokens: &[Token], locator: &Locator) -> Indentation {
    let indent_range = tokens.iter().find_map(|token| {
        if matches!(token.kind(), TokenKind::Indent) {
            Some(token.range())
        } else {
            None
        }
    });

    if let Some(indent_range) = indent_range {
        let mut whitespace = locator.slice(indent_range);
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
        // If we can't find a logical indent token, search for a non-logical indent, to cover cases
        // like:
        //```python
        // from math import (
        //   sin,
        //   tan,
        //   cos,
        // )
        // ```
        for token in tokens {
            if token.kind() == TokenKind::NonLogicalNewline {
                let line = locator.line(token.end());
                let indent_index = line.find(|c: char| !c.is_whitespace());
                if let Some(indent_index) = indent_index {
                    if indent_index > 0 {
                        let whitespace = &line[..indent_index];
                        return Indentation(whitespace.to_string());
                    }
                }
            }
        }

        Indentation::default()
    }
}

/// The indentation style used in Python source code.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    use ruff_python_parser::{parse_module, parse_unchecked, Mode};

    use ruff_source_file::{find_newline, LineEnding};

    use super::{Indentation, Quote, Stylist};
    use ruff_source_file::Locator;

    #[test]
    fn indentation() {
        let contents = r"x = 1";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.indentation(), &Indentation::default());

        let contents = r"
if True:
  pass
";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.indentation(), &Indentation("  ".to_string()));

        let contents = r"
if True:
    pass
";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.indentation(), &Indentation("    ".to_string()));

        let contents = r"
if True:
	pass
";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.indentation(), &Indentation("\t".to_string()));

        let contents = r"
x = (
  1,
  2,
  3,
)
";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.indentation(), &Indentation("  ".to_string()));

        // formfeed indent, see `detect_indentation` comment.
        let contents = r"
class FormFeedIndent:
   def __init__(self, a=[]):
        print(a)
";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.indentation(), &Indentation(" ".to_string()));
    }

    #[test]
    fn indent_non_breaking_whitespace() {
        let contents = r"
x = (
 1,
 2,
 3,
)
";
        let locator = Locator::new(contents);
        let parsed = parse_unchecked(contents, Mode::Module);
        assert_eq!(
            Stylist::from_tokens(parsed.tokens(), &locator).indentation(),
            &Indentation(" ".to_string())
        );
    }

    #[test]
    fn quote() {
        let contents = r"x = 1";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::default());

        let contents = r"x = '1'";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Single);

        let contents = r"x = f'1'";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Single);

        let contents = r#"x = "1""#;
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Double);

        let contents = r#"x = f"1""#;
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Double);

        let contents = r#"s = "It's done.""#;
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Double);

        // No style if only double quoted docstring (will take default Double)
        let contents = r#"
def f():
    """Docstring."""
    pass
"#;
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::default());

        // Detect from string literal appearing after docstring
        let contents = r#"
"""Module docstring."""

a = 'v'
"#;
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Single);

        let contents = r#"
'''Module docstring.'''

a = "v"
"#;
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Double);

        // Detect from f-string appearing after docstring
        let contents = r#"
"""Module docstring."""

a = f'v'
"#;
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Single);

        let contents = r#"
'''Module docstring.'''

a = f"v"
"#;
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Double);

        let contents = r"
f'''Module docstring.'''
";
        let locator = Locator::new(contents);
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
        assert_eq!(stylist.quote(), Quote::Single);
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
