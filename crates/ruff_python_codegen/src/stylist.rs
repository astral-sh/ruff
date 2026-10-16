//! Detect code style from Python source code.

use std::ops::Deref;
use std::sync::OnceLock;

use ruff_python_ast::str::Quote;
use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_source_file::{LineEnding, LineRanges, find_newline};
use ruff_text_size::Ranged;

#[derive(Debug, Clone)]
pub struct Stylist<'a> {
    indentation: Indentation,
    quote: Quote,
    line_ending: LineEndingDetection<'a>,
}

impl<'a> Stylist<'a> {
    pub fn indentation(&self) -> &Indentation {
        &self.indentation
    }

    pub fn quote(&self) -> Quote {
        self.quote
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending.get()
    }

    pub fn into_owned(self) -> Stylist<'static> {
        Stylist {
            indentation: self.indentation,
            quote: self.quote,
            line_ending: LineEndingDetection::Detected(self.line_ending.get()),
        }
    }

    pub fn from_tokens(tokens: &Tokens, source: &'a str) -> Self {
        let indentation = detect_indentation(tokens, source);

        Self {
            indentation,
            quote: detect_quote(tokens),
            line_ending: LineEndingDetection::Lazy {
                source,
                detected: OnceLock::new(),
            },
        }
    }
}

/// Borrowed stylists detect line endings on demand. Owned stylists keep only the result,
/// so they do not need to retain or copy the source text.
#[derive(Debug, Clone)]
enum LineEndingDetection<'a> {
    Lazy {
        source: &'a str,
        detected: OnceLock<LineEnding>,
    },
    Detected(LineEnding),
}

impl LineEndingDetection<'_> {
    fn get(&self) -> LineEnding {
        match self {
            Self::Lazy { source, detected } => *detected.get_or_init(|| {
                find_newline(source)
                    .map(|(_, ending)| ending)
                    .unwrap_or_default()
            }),
            Self::Detected(line_ending) => *line_ending,
        }
    }
}

fn detect_quote(tokens: &[Token]) -> Quote {
    for token in tokens {
        match token.kind() {
            TokenKind::String if !token.is_triple_quoted_string() => {
                return token.string_quote_style();
            }
            TokenKind::FStringStart => return token.string_quote_style(),
            _ => continue,
        }
    }
    Quote::default()
}

fn detect_indentation(tokens: &[Token], source: &str) -> Indentation {
    let indent_range = tokens.iter().find_map(|token| {
        if matches!(token.kind(), TokenKind::Indent) {
            Some(token.range())
        } else {
            None
        }
    });

    if let Some(indent_range) = indent_range {
        let mut whitespace = &source[indent_range];
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
                let line = source.line_str(token.end());
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
    use ruff_python_parser::{Mode, ParseOptions, parse_module, parse_unchecked};
    use ruff_source_file::{LineEnding, find_newline};

    use super::{Indentation, Quote, Stylist};

    #[test]
    fn owned_style_preserves_source_style() {
        let stylist = {
            let source = String::from("def f():\r\n  return 'value'\r\n");
            let parsed = parse_module(&source).unwrap();
            Stylist::from_tokens(parsed.tokens(), &source).into_owned()
        };

        assert_eq!(stylist.indentation().as_str(), "  ");
        assert_eq!(stylist.quote(), Quote::Single);
        assert_eq!(stylist.line_ending(), LineEnding::CrLf);
    }

    #[test]
    fn indentation() {
        let contents = r"x = 1";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.indentation(), &Indentation::default());

        let contents = r"
if True:
  pass
";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.indentation(), &Indentation("  ".to_string()));

        let contents = r"
if True:
    pass
";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.indentation(), &Indentation("    ".to_string()));

        let contents = r"
if True:
	pass
";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.indentation(), &Indentation("\t".to_string()));

        let contents = r"
x = (
  1,
  2,
  3,
)
";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.indentation(), &Indentation("  ".to_string()));

        // formfeed indent, see `detect_indentation` comment.
        let contents = r"
class FormFeedIndent:
   def __init__(self, a=[]):
        print(a)
";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
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
        let parsed = parse_unchecked(contents, ParseOptions::from(Mode::Module));
        assert_eq!(
            Stylist::from_tokens(parsed.tokens(), contents).indentation(),
            &Indentation(" ".to_string())
        );
    }

    #[test]
    fn quote() {
        let contents = r"x = 1";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::default());

        let contents = r"x = '1'";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Single);

        let contents = r"x = f'1'";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Single);

        let contents = r#"x = "1""#;
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Double);

        let contents = r#"x = f"1""#;
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Double);

        let contents = r#"s = "It's done.""#;
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Double);

        // No style if only double quoted docstring (will take default Double)
        let contents = r#"
def f():
    """Docstring."""
    pass
"#;
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::default());

        // Detect from string literal appearing after docstring
        let contents = r#"
"""Module docstring."""

a = 'v'
"#;
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Single);

        let contents = r#"
'''Module docstring.'''

a = "v"
"#;
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Double);

        // Detect from f-string appearing after docstring
        let contents = r#"
"""Module docstring."""

a = f'v'
"#;
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Single);

        let contents = r#"
'''Module docstring.'''

a = f"v"
"#;
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
        assert_eq!(stylist.quote(), Quote::Double);

        let contents = r"
f'''Module docstring.'''
";
        let parsed = parse_module(contents).unwrap();
        let stylist = Stylist::from_tokens(parsed.tokens(), contents);
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
