use ruff_python_ast::ConversionFlag;
use ruff_python_ast::{self as ast, BytesConstant, Constant, Expr, StringConstant};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

// Contains the logic for parsing string literals (mostly concerned with f-strings.)
//
// The lexer doesn't do any special handling of f-strings, it just treats them as
// regular strings. Since the ruff_python_parser has no definition of f-string formats (Pending PEP 701)
// we have to do the parsing here, manually.
use crate::{
    lexer::{LexicalError, LexicalErrorType},
    parse_expression_starts_at,
    parser::{ParseError, ParseErrorType},
    token::{StringKind, Tok},
};

// unicode_name2 does not expose `MAX_NAME_LENGTH`, so we replicate that constant here, fix #3798
const MAX_UNICODE_NAME: usize = 88;

struct StringParser<'a> {
    chars: std::str::Chars<'a>,
    kind: StringKind,
    location: TextSize,
}

impl<'a> StringParser<'a> {
    fn new(source: &'a str, kind: StringKind, triple_quoted: bool, start: TextSize) -> Self {
        let offset = kind.prefix_len()
            + if triple_quoted {
                TextSize::from(3)
            } else {
                TextSize::from(1)
            };
        Self {
            chars: source.chars(),
            kind,
            location: start + offset,
        }
    }

    #[inline]
    fn next_char(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.location += c.text_len();
        Some(c)
    }

    #[inline]
    fn peek(&mut self) -> Option<char> {
        self.chars.clone().next()
    }

    #[inline]
    fn peek2(&mut self) -> Option<char> {
        let mut chars = self.chars.clone();
        chars.next();
        chars.next()
    }

    #[inline]
    fn get_pos(&self) -> TextSize {
        self.location
    }

    #[inline]
    fn range(&self, start_location: TextSize) -> TextRange {
        TextRange::new(start_location, self.location)
    }

    fn parse_unicode_literal(&mut self, literal_number: usize) -> Result<char, LexicalError> {
        let mut p: u32 = 0u32;
        let unicode_error = LexicalError::new(LexicalErrorType::UnicodeError, self.get_pos());
        for i in 1..=literal_number {
            match self.next_char() {
                Some(c) => match c.to_digit(16) {
                    Some(d) => p += d << ((literal_number - i) * 4),
                    None => return Err(unicode_error),
                },
                None => return Err(unicode_error),
            }
        }
        match p {
            0xD800..=0xDFFF => Ok(std::char::REPLACEMENT_CHARACTER),
            _ => std::char::from_u32(p).ok_or(unicode_error),
        }
    }

    fn parse_octet(&mut self, first: char) -> char {
        let mut octet_content = String::new();
        octet_content.push(first);
        while octet_content.len() < 3 {
            if let Some('0'..='7') = self.peek() {
                octet_content.push(self.next_char().unwrap());
            } else {
                break;
            }
        }
        let value = u32::from_str_radix(&octet_content, 8).unwrap();
        char::from_u32(value).unwrap()
    }

    fn parse_unicode_name(&mut self) -> Result<char, LexicalError> {
        let start_pos = self.get_pos();
        match self.next_char() {
            Some('{') => {}
            _ => return Err(LexicalError::new(LexicalErrorType::StringError, start_pos)),
        }
        let start_pos = self.get_pos();
        let mut name = String::new();
        loop {
            match self.next_char() {
                Some('}') => break,
                Some(c) => name.push(c),
                None => {
                    return Err(LexicalError::new(
                        LexicalErrorType::StringError,
                        self.get_pos(),
                    ))
                }
            }
        }

        if name.len() > MAX_UNICODE_NAME {
            return Err(LexicalError::new(
                LexicalErrorType::UnicodeError,
                self.get_pos(),
            ));
        }

        unicode_names2::character(&name)
            .ok_or_else(|| LexicalError::new(LexicalErrorType::UnicodeError, start_pos))
    }

    fn parse_escaped_char(&mut self) -> Result<String, LexicalError> {
        match self.next_char() {
            Some(c) => {
                let char = match c {
                    '\\' => '\\',
                    '\'' => '\'',
                    '\"' => '"',
                    'a' => '\x07',
                    'b' => '\x08',
                    'f' => '\x0c',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'v' => '\x0b',
                    o @ '0'..='7' => self.parse_octet(o),
                    'x' => self.parse_unicode_literal(2)?,
                    'u' if !self.kind.is_any_bytes() => self.parse_unicode_literal(4)?,
                    'U' if !self.kind.is_any_bytes() => self.parse_unicode_literal(8)?,
                    'N' if !self.kind.is_any_bytes() => self.parse_unicode_name()?,
                    // Special cases where the escape sequence is not a single character
                    '\n' => return Ok(String::new()),
                    c => {
                        if self.kind.is_any_bytes() && !c.is_ascii() {
                            return Err(LexicalError {
                                error: LexicalErrorType::OtherError(
                                    "bytes can only contain ASCII literal characters".to_owned(),
                                ),
                                location: self.get_pos(),
                            });
                        }
                        return Ok(format!("\\{c}"));
                    }
                };
                Ok(char.to_string())
            }
            None => Err(LexicalError {
                error: LexicalErrorType::StringError,
                location: self.get_pos(),
            }),
        }
    }

    fn parse_formatted_value(&mut self, nested: u8) -> Result<Vec<Expr>, LexicalError> {
        use FStringErrorType::{
            EmptyExpression, InvalidConversionFlag, InvalidExpression, MismatchedDelimiter,
            UnclosedLbrace, Unmatched, UnterminatedString,
        };

        let mut expression = String::new();
        // for self-documenting strings we also store the `=` and any trailing space inside
        // expression (because we want to combine it with any trailing spaces before the equal
        // sign). the expression_length is the length of the actual expression part that we pass to
        // `parse_fstring_expr`
        let mut expression_length = 0;
        let mut spec = None;
        let mut delimiters = Vec::new();
        let mut conversion = ConversionFlag::None;
        let mut self_documenting = false;
        let start_location = self.get_pos();

        assert_eq!(self.next_char(), Some('{'));

        while let Some(ch) = self.next_char() {
            match ch {
                // can be integrated better with the remaining code, but as a starting point ok
                // in general I would do here a tokenizing of the fstrings to omit this peeking.
                '!' | '=' | '>' | '<' if self.peek() == Some('=') => {
                    expression.push(ch);
                    expression.push('=');
                    self.next_char();
                }
                '!' if delimiters.is_empty() && self.peek() != Some('=') => {
                    if expression.trim().is_empty() {
                        return Err(FStringError::new(EmptyExpression, self.get_pos()).into());
                    }

                    conversion = match self.next_char() {
                        Some('s') => ConversionFlag::Str,
                        Some('a') => ConversionFlag::Ascii,
                        Some('r') => ConversionFlag::Repr,
                        Some(_) => {
                            return Err(
                                FStringError::new(InvalidConversionFlag, self.get_pos()).into()
                            );
                        }
                        None => {
                            return Err(FStringError::new(UnclosedLbrace, self.get_pos()).into());
                        }
                    };

                    match self.peek() {
                        Some('}' | ':') => {}
                        Some(_) | None => {
                            return Err(FStringError::new(UnclosedLbrace, self.get_pos()).into());
                        }
                    }
                }

                // match a python 3.8 self documenting expression
                // format '{' PYTHON_EXPRESSION '=' FORMAT_SPECIFIER? '}'
                '=' if self.peek() != Some('=') && delimiters.is_empty() => {
                    expression_length = expression.len();
                    expression.push(ch);
                    self_documenting = true;
                }

                ':' if delimiters.is_empty() => {
                    let start_location = self.get_pos();
                    let parsed_spec = self.parse_spec(nested)?;

                    spec = Some(Box::new(Expr::from(ast::ExprFString {
                        values: parsed_spec,
                        implicit_concatenated: false,
                        range: self.range(start_location),
                    })));
                }
                '(' | '{' | '[' => {
                    expression.push(ch);
                    delimiters.push(ch);
                }
                ')' => {
                    let last_delim = delimiters.pop();
                    match last_delim {
                        Some('(') => {
                            expression.push(ch);
                        }
                        Some(c) => {
                            return Err(FStringError::new(
                                MismatchedDelimiter(c, ')'),
                                self.get_pos(),
                            )
                            .into());
                        }
                        None => {
                            return Err(FStringError::new(Unmatched(')'), self.get_pos()).into());
                        }
                    }
                }
                ']' => {
                    let last_delim = delimiters.pop();
                    match last_delim {
                        Some('[') => {
                            expression.push(ch);
                        }
                        Some(c) => {
                            return Err(FStringError::new(
                                MismatchedDelimiter(c, ']'),
                                self.get_pos(),
                            )
                            .into());
                        }
                        None => {
                            return Err(FStringError::new(Unmatched(']'), self.get_pos()).into());
                        }
                    }
                }
                '}' if !delimiters.is_empty() => {
                    let last_delim = delimiters.pop();
                    match last_delim {
                        Some('{') => {
                            expression.push(ch);
                        }
                        Some(c) => {
                            return Err(FStringError::new(
                                MismatchedDelimiter(c, '}'),
                                self.get_pos(),
                            )
                            .into());
                        }
                        None => {}
                    }
                }
                '}' => {
                    if expression.trim().is_empty() {
                        return Err(FStringError::new(EmptyExpression, self.get_pos()).into());
                    }

                    let ret = if self_documenting {
                        let value =
                            parse_fstring_expr(&expression[..expression_length], start_location)
                                .map_err(|e| {
                                    FStringError::new(
                                        InvalidExpression(Box::new(e.error)),
                                        start_location,
                                    )
                                })?;
                        let leading =
                            &expression[..usize::from(value.start() - start_location) - 1];
                        let trailing = &expression[usize::from(value.end() - start_location) - 1..];
                        vec![Expr::from(ast::ExprFormattedValue {
                            value: Box::new(value),
                            debug_text: Some(ast::DebugText {
                                leading: leading.to_string(),
                                trailing: trailing.to_string(),
                            }),
                            conversion,
                            format_spec: spec,
                            range: self.range(start_location),
                        })]
                    } else {
                        vec![Expr::from(ast::ExprFormattedValue {
                            value: Box::new(
                                parse_fstring_expr(&expression, start_location).map_err(|e| {
                                    FStringError::new(
                                        InvalidExpression(Box::new(e.error)),
                                        start_location,
                                    )
                                })?,
                            ),
                            debug_text: None,
                            conversion,
                            format_spec: spec,
                            range: self.range(start_location),
                        })]
                    };
                    return Ok(ret);
                }
                '"' | '\'' => {
                    expression.push(ch);
                    loop {
                        let Some(c) = self.next_char() else {
                            return Err(
                                FStringError::new(UnterminatedString, self.get_pos()).into()
                            );
                        };
                        expression.push(c);
                        if c == ch {
                            break;
                        }
                    }
                }
                ' ' if self_documenting => expression.push(ch),
                '\\' => return Err(FStringError::new(UnterminatedString, self.get_pos()).into()),
                _ => {
                    if self_documenting {
                        return Err(FStringError::new(UnclosedLbrace, self.get_pos()).into());
                    }

                    expression.push(ch);
                }
            }
        }
        Err(FStringError::new(UnclosedLbrace, self.get_pos()).into())
    }

    fn parse_spec(&mut self, nested: u8) -> Result<Vec<Expr>, LexicalError> {
        let mut spec_constructor = Vec::new();
        let mut constant_piece = String::new();
        let mut start_location = self.get_pos();
        while let Some(next) = self.peek() {
            match next {
                '{' => {
                    if !constant_piece.is_empty() {
                        spec_constructor.push(Expr::from(ast::ExprConstant {
                            value: std::mem::take(&mut constant_piece).into(),
                            range: self.range(start_location),
                        }));
                    }
                    let parsed_expr = self.parse_fstring(nested + 1)?;
                    spec_constructor.extend(parsed_expr);
                    start_location = self.get_pos();
                    continue;
                }
                '}' => {
                    break;
                }
                _ => {
                    constant_piece.push(next);
                }
            }
            self.next_char();
        }
        if !constant_piece.is_empty() {
            spec_constructor.push(Expr::from(ast::ExprConstant {
                value: std::mem::take(&mut constant_piece).into(),
                range: self.range(start_location),
            }));
        }
        Ok(spec_constructor)
    }

    fn parse_fstring(&mut self, nested: u8) -> Result<Vec<Expr>, LexicalError> {
        use FStringErrorType::{ExpressionNestedTooDeeply, SingleRbrace, UnclosedLbrace};

        if nested >= 2 {
            return Err(FStringError::new(ExpressionNestedTooDeeply, self.get_pos()).into());
        }

        let mut content = String::new();
        let mut start_location = self.get_pos();
        let mut values = vec![];

        while let Some(ch) = self.peek() {
            match ch {
                '{' => {
                    if nested == 0 {
                        match self.peek2() {
                            Some('{') => {
                                self.next_char();
                                self.next_char();
                                content.push('{');
                                continue;
                            }
                            None => {
                                return Err(FStringError::new(UnclosedLbrace, self.get_pos()).into())
                            }
                            _ => {}
                        }
                    }
                    if !content.is_empty() {
                        values.push(Expr::from(ast::ExprConstant {
                            value: std::mem::take(&mut content).into(),
                            range: self.range(start_location),
                        }));
                    }

                    let parsed_values = self.parse_formatted_value(nested)?;
                    values.extend(parsed_values);
                    start_location = self.get_pos();
                }
                '}' => {
                    if nested > 0 {
                        break;
                    }
                    self.next_char();
                    if let Some('}') = self.peek() {
                        self.next_char();
                        content.push('}');
                    } else {
                        return Err(FStringError::new(SingleRbrace, self.get_pos()).into());
                    }
                }
                '\\' if !self.kind.is_raw() => {
                    self.next_char();
                    content.push_str(&self.parse_escaped_char()?);
                }
                _ => {
                    content.push(ch);
                    self.next_char();
                }
            }
        }

        if !content.is_empty() {
            values.push(Expr::from(ast::ExprConstant {
                value: content.into(),
                range: self.range(start_location),
            }));
        }

        Ok(values)
    }

    fn parse_bytes(&mut self) -> Result<Expr, LexicalError> {
        let mut content = String::new();
        let start_location = self.get_pos();
        while let Some(ch) = self.next_char() {
            match ch {
                '\\' if !self.kind.is_raw() => {
                    content.push_str(&self.parse_escaped_char()?);
                }
                ch => {
                    if !ch.is_ascii() {
                        return Err(LexicalError::new(
                            LexicalErrorType::OtherError(
                                "bytes can only contain ASCII literal characters".to_string(),
                            ),
                            self.get_pos(),
                        ));
                    }
                    content.push(ch);
                }
            }
        }

        Ok(Expr::from(ast::ExprConstant {
            value: content.chars().map(|c| c as u8).collect::<Vec<u8>>().into(),
            range: self.range(start_location),
        }))
    }

    fn parse_string(&mut self) -> Result<Expr, LexicalError> {
        let mut value = String::new();
        let start_location = self.get_pos();
        while let Some(ch) = self.next_char() {
            match ch {
                '\\' if !self.kind.is_raw() => {
                    value.push_str(&self.parse_escaped_char()?);
                }
                ch => value.push(ch),
            }
        }
        Ok(Expr::from(ast::ExprConstant {
            value: ast::Constant::Str(ast::StringConstant {
                value,
                unicode: self.kind.is_unicode(),
                implicit_concatenated: false,
            }),
            range: self.range(start_location),
        }))
    }

    fn parse(&mut self) -> Result<Vec<Expr>, LexicalError> {
        if self.kind.is_any_fstring() {
            self.parse_fstring(0)
        } else if self.kind.is_any_bytes() {
            self.parse_bytes().map(|expr| vec![expr])
        } else {
            self.parse_string().map(|expr| vec![expr])
        }
    }
}

fn parse_fstring_expr(source: &str, location: TextSize) -> Result<Expr, ParseError> {
    let fstring_body = format!("({source})");
    parse_expression_starts_at(&fstring_body, "<fstring>", location)
}

fn parse_string(
    source: &str,
    kind: StringKind,
    triple_quoted: bool,
    start: TextSize,
) -> Result<Vec<Expr>, LexicalError> {
    StringParser::new(source, kind, triple_quoted, start).parse()
}

pub(crate) fn parse_strings(
    values: Vec<(TextSize, (String, StringKind, bool), TextSize)>,
) -> Result<Expr, LexicalError> {
    // Preserve the initial location and kind.
    let initial_start = values[0].0;
    let last_end = values.last().unwrap().2;
    let is_initial_kind_unicode = values[0].1 .1 == StringKind::Unicode;
    let has_fstring = values
        .iter()
        .any(|(_, (_, kind, ..), _)| kind.is_any_fstring());
    let num_bytes = values
        .iter()
        .filter(|(_, (_, kind, ..), _)| kind.is_any_bytes())
        .count();
    let has_bytes = num_bytes > 0;
    let implicit_concatenated = values.len() > 1;

    if has_bytes && num_bytes < values.len() {
        return Err(LexicalError {
            error: LexicalErrorType::OtherError(
                "cannot mix bytes and nonbytes literals".to_owned(),
            ),
            location: initial_start,
        });
    }

    if has_bytes {
        let mut content: Vec<u8> = vec![];
        for (start, (source, kind, triple_quoted), _) in values {
            for value in parse_string(&source, kind, triple_quoted, start)? {
                match value {
                    Expr::Constant(ast::ExprConstant {
                        value: Constant::Bytes(BytesConstant { value, .. }),
                        ..
                    }) => content.extend(value),
                    _ => unreachable!("Unexpected non-bytes expression."),
                }
            }
        }
        return Ok(ast::ExprConstant {
            value: Constant::Bytes(BytesConstant {
                value: content,
                implicit_concatenated,
            }),
            range: TextRange::new(initial_start, last_end),
        }
        .into());
    }

    if !has_fstring {
        let mut content: Vec<String> = vec![];
        for (start, (source, kind, triple_quoted), _) in values {
            for value in parse_string(&source, kind, triple_quoted, start)? {
                match value {
                    Expr::Constant(ast::ExprConstant {
                        value: Constant::Str(StringConstant { value, .. }),
                        ..
                    }) => content.push(value),
                    _ => unreachable!("Unexpected non-string expression."),
                }
            }
        }
        return Ok(ast::ExprConstant {
            value: Constant::Str(StringConstant {
                value: content.join(""),
                unicode: is_initial_kind_unicode,
                implicit_concatenated,
            }),
            range: TextRange::new(initial_start, last_end),
        }
        .into());
    }

    // De-duplicate adjacent constants.
    let mut deduped: Vec<Expr> = vec![];
    let mut current: Vec<String> = vec![];
    let mut current_start = initial_start;
    let mut current_end = last_end;

    let take_current = |current: &mut Vec<String>, start, end| -> Expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(StringConstant {
                value: current.drain(..).collect::<String>(),
                unicode: is_initial_kind_unicode,
                implicit_concatenated,
            }),
            range: TextRange::new(start, end),
        })
    };

    for (start, (source, kind, triple_quoted), _) in values {
        for value in parse_string(&source, kind, triple_quoted, start)? {
            let value_range = value.range();
            match value {
                Expr::FormattedValue { .. } => {
                    if !current.is_empty() {
                        deduped.push(take_current(&mut current, current_start, current_end));
                    }
                    deduped.push(value);
                }
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(StringConstant { value, .. }),
                    ..
                }) => {
                    if current.is_empty() {
                        current_start = value_range.start();
                    }
                    current_end = value_range.end();
                    current.push(value);
                }
                _ => unreachable!("Unexpected non-string expression."),
            }
        }
    }
    if !current.is_empty() {
        deduped.push(take_current(&mut current, current_start, current_end));
    }

    Ok(Expr::FString(ast::ExprFString {
        values: deduped,
        implicit_concatenated,
        range: TextRange::new(initial_start, last_end),
    }))
}

// TODO: consolidate these with ParseError
/// An error that occurred during parsing of an f-string.
#[derive(Debug, PartialEq)]
struct FStringError {
    /// The type of error that occurred.
    pub(crate) error: FStringErrorType,
    /// The location of the error.
    pub(crate) location: TextSize,
}

impl FStringError {
    /// Creates a new `FStringError` with the given error type and location.
    pub(crate) fn new(error: FStringErrorType, location: TextSize) -> Self {
        Self { error, location }
    }
}

impl From<FStringError> for LexicalError {
    fn from(err: FStringError) -> Self {
        LexicalError {
            error: LexicalErrorType::FStringError(err.error),
            location: err.location,
        }
    }
}

/// Represents the different types of errors that can occur during parsing of an f-string.
#[derive(Debug, PartialEq)]
pub enum FStringErrorType {
    /// Expected a right brace after an opened left brace.
    UnclosedLbrace,
    /// An error occurred while parsing an f-string expression.
    InvalidExpression(Box<ParseErrorType>),
    /// An invalid conversion flag was encountered.
    InvalidConversionFlag,
    /// An empty expression was encountered.
    EmptyExpression,
    /// An opening delimiter was not closed properly.
    MismatchedDelimiter(char, char),
    /// Too many nested expressions in an f-string.
    ExpressionNestedTooDeeply,
    /// The f-string expression cannot include the given character.
    ExpressionCannotInclude(char),
    /// A single right brace was encountered.
    SingleRbrace,
    /// A closing delimiter was not opened properly.
    Unmatched(char),
    // TODO: Test this case.
    /// Unterminated string.
    UnterminatedString,
}

impl std::fmt::Display for FStringErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use FStringErrorType::{
            EmptyExpression, ExpressionCannotInclude, ExpressionNestedTooDeeply,
            InvalidConversionFlag, InvalidExpression, MismatchedDelimiter, SingleRbrace,
            UnclosedLbrace, Unmatched, UnterminatedString,
        };
        match self {
            UnclosedLbrace => write!(f, "expecting '}}'"),
            InvalidExpression(error) => {
                write!(f, "{error}")
            }
            InvalidConversionFlag => write!(f, "invalid conversion character"),
            EmptyExpression => write!(f, "empty expression not allowed"),
            MismatchedDelimiter(first, second) => write!(
                f,
                "closing parenthesis '{second}' does not match opening parenthesis '{first}'"
            ),
            SingleRbrace => write!(f, "single '}}' is not allowed"),
            Unmatched(delim) => write!(f, "unmatched '{delim}'"),
            ExpressionNestedTooDeeply => {
                write!(f, "expressions nested too deeply")
            }
            UnterminatedString => {
                write!(f, "unterminated string")
            }
            ExpressionCannotInclude(c) => {
                if *c == '\\' {
                    write!(f, "f-string expression part cannot include a backslash")
                } else {
                    write!(f, "f-string expression part cannot include '{c}'s")
                }
            }
        }
    }
}

impl From<FStringError> for crate::parser::LalrpopError<TextSize, Tok, LexicalError> {
    fn from(err: FStringError) -> Self {
        lalrpop_util::ParseError::User {
            error: LexicalError {
                error: LexicalErrorType::FStringError(err.error),
                location: err.location,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_suite;

    fn parse_fstring(source: &str) -> Result<Vec<Expr>, LexicalError> {
        StringParser::new(source, StringKind::FString, false, TextSize::default()).parse()
    }

    #[test]
    fn test_parse_fstring() {
        let source = "{a}{ b }{{foo}}";
        let parse_ast = parse_fstring(source).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_nested_spec() {
        let source = "{foo:{spec}}";
        let parse_ast = parse_fstring(source).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_not_nested_spec() {
        let source = "{foo:spec}";
        let parse_ast = parse_fstring(source).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_empty_fstring() {
        insta::assert_debug_snapshot!(parse_fstring("").unwrap());
    }

    #[test]
    fn test_fstring_parse_self_documenting_base() {
        let src = "{user=}";
        let parse_ast = parse_fstring(src).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_parse_self_documenting_base_more() {
        let src = "mix {user=} with text and {second=}";
        let parse_ast = parse_fstring(src).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_parse_self_documenting_format() {
        let src = "{user=:>10}";
        let parse_ast = parse_fstring(src).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    fn parse_fstring_error(source: &str) -> FStringErrorType {
        parse_fstring(source)
            .map_err(|e| match e.error {
                LexicalErrorType::FStringError(e) => e,
                e => unreachable!("Expected FStringError: {:?}", e),
            })
            .expect_err("Expected error")
    }

    #[test]
    fn test_parse_invalid_fstring() {
        use FStringErrorType::{
            EmptyExpression, ExpressionNestedTooDeeply, InvalidConversionFlag, SingleRbrace,
            UnclosedLbrace,
        };
        assert_eq!(parse_fstring_error("{5!a"), UnclosedLbrace);
        assert_eq!(parse_fstring_error("{5!a1}"), UnclosedLbrace);
        assert_eq!(parse_fstring_error("{5!"), UnclosedLbrace);
        assert_eq!(parse_fstring_error("abc{!a 'cat'}"), EmptyExpression);
        assert_eq!(parse_fstring_error("{!a"), EmptyExpression);
        assert_eq!(parse_fstring_error("{ !a}"), EmptyExpression);

        assert_eq!(parse_fstring_error("{5!}"), InvalidConversionFlag);
        assert_eq!(parse_fstring_error("{5!x}"), InvalidConversionFlag);

        assert_eq!(
            parse_fstring_error("{a:{a:{b}}}"),
            ExpressionNestedTooDeeply
        );

        assert_eq!(parse_fstring_error("{a:b}}"), SingleRbrace);
        assert_eq!(parse_fstring_error("}"), SingleRbrace);
        assert_eq!(parse_fstring_error("{a:{b}"), UnclosedLbrace);
        assert_eq!(parse_fstring_error("{"), UnclosedLbrace);

        assert_eq!(parse_fstring_error("{}"), EmptyExpression);

        // TODO: check for InvalidExpression enum?
        assert!(parse_fstring("{class}").is_err());
    }

    #[test]
    fn test_parse_fstring_not_equals() {
        let source = "{1 != 2}";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_equals() {
        let source = "{42 == 42}";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_self_doc_prec_space() {
        let source = "{x   =}";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_self_doc_trailing_space() {
        let source = "{x=   }";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_yield_expr() {
        let source = "{yield}";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_string_concat() {
        let source = "'Hello ' 'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_string_concat_1() {
        let source = "'Hello ' u'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_string_concat_2() {
        let source = "u'Hello ' 'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_1() {
        let source = "'Hello ' f'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_2() {
        let source = "'Hello ' f'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_3() {
        let source = "'Hello ' f'world{\"!\"}'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_4() {
        let source = "'Hello ' f'world{\"!\"}' 'again!'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_f_string_concat_1() {
        let source = "u'Hello ' f'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_f_string_concat_2() {
        let source = "u'Hello ' f'world' '!'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_string_triple_quotes_with_kind() {
        let source = "u'''Hello, world!'''";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_single_quoted_byte() {
        // single quote
        let source = r##"b'\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff'"##;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_double_quoted_byte() {
        // double quote
        let source = r##"b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff""##;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_escape_char_in_byte_literal() {
        // backslash does not escape
        let source = r#"b"omkmok\Xaa""#; // spell-checker:ignore omkmok
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_byte_literal_1() {
        let source = r"rb'\x1z'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_byte_literal_2() {
        let source = r"rb'\\'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_escape_octet() {
        let source = r"b'\43a\4\1234'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_escaped_newline() {
        let source = r#"f"\n{x}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_constant_range() {
        let source = r#"f"aaa{bbb}ccc{ddd}eee""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_unescaped_newline() {
        let source = r#"f"""
{x}""""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_escaped_character() {
        let source = r#"f"\\{x}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_fstring() {
        let source = r#"rf"{x}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_triple_quoted_raw_fstring() {
        let source = r#"rf"""{x}""""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_line_continuation() {
        let source = r#"rf"\
{x}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_nested_string_spec() {
        let source = "{foo:{''}}";
        let parse_ast = parse_fstring(source).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_nested_concatenation_string_spec() {
        let source = "{foo:{'' ''}}";
        let parse_ast = parse_fstring(source).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    macro_rules! test_aliases_parse {
        ($($name:ident: $alias:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let source = format!(r#""\N{{{0}}}""#, $alias);
                let parse_ast = parse_suite(&source, "<test>").unwrap();
                insta::assert_debug_snapshot!(parse_ast);
            }
        )*
        }
    }

    test_aliases_parse! {
        test_backspace_alias: "BACKSPACE",
        test_bell_alias: "BEL",
        test_carriage_return_alias: "CARRIAGE RETURN",
        test_delete_alias: "DELETE",
        test_escape_alias: "ESCAPE",
        test_form_feed_alias: "FORM FEED",
        test_hts_alias: "HTS",
        test_character_tabulation_with_justification_alias: "CHARACTER TABULATION WITH JUSTIFICATION",
    }
}
