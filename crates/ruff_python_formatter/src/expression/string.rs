use std::borrow::Cow;

use bitflags::bitflags;

use ruff_formatter::{format_args, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{
    self as ast, ExprBytesLiteral, ExprFString, ExprStringLiteral, ExpressionRef,
};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break_or_space,
};
use crate::expression::Expr;
use crate::prelude::*;
use crate::QuoteStyle;

#[derive(Copy, Clone)]
enum Quoting {
    CanChange,
    Preserve,
}

#[derive(Clone, Debug)]
pub(super) enum AnyString<'a> {
    String(&'a ExprStringLiteral),
    Bytes(&'a ExprBytesLiteral),
    FString(&'a ExprFString),
}

impl<'a> AnyString<'a> {
    pub(crate) fn from_expression(expression: &'a Expr) -> Option<AnyString<'a>> {
        match expression {
            Expr::StringLiteral(string) => Some(AnyString::String(string)),
            Expr::BytesLiteral(bytes) => Some(AnyString::Bytes(bytes)),
            Expr::FString(fstring) => Some(AnyString::FString(fstring)),
            _ => None,
        }
    }

    fn quoting(&self, locator: &Locator) -> Quoting {
        match self {
            Self::String(_) | Self::Bytes(_) => Quoting::CanChange,
            Self::FString(f_string) => {
                let unprefixed = locator
                    .slice(f_string.range)
                    .trim_start_matches(|c| c != '"' && c != '\'');
                let triple_quoted =
                    unprefixed.starts_with(r#"""""#) || unprefixed.starts_with(r"'''");
                if f_string.value.elements().any(|value| match value {
                    Expr::FormattedValue(ast::ExprFormattedValue { range, .. }) => {
                        let string_content = locator.slice(*range);
                        if triple_quoted {
                            string_content.contains(r#"""""#) || string_content.contains("'''")
                        } else {
                            string_content.contains(['"', '\''])
                        }
                    }
                    _ => false,
                }) {
                    Quoting::Preserve
                } else {
                    Quoting::CanChange
                }
            }
        }
    }

    /// Returns `true` if the string is implicitly concatenated.
    pub(super) fn is_implicit_concatenated(&self) -> bool {
        match self {
            Self::String(ExprStringLiteral { value, .. }) => value.is_implicit_concatenated(),
            Self::Bytes(ExprBytesLiteral { value, .. }) => value.is_implicit_concatenated(),
            Self::FString(ExprFString { value, .. }) => value.is_implicit_concatenated(),
        }
    }

    fn parts(&self) -> Vec<AnyStringPart<'a>> {
        match self {
            Self::String(ExprStringLiteral { value, .. }) => {
                value.parts().map(AnyStringPart::String).collect()
            }
            Self::Bytes(ExprBytesLiteral { value, .. }) => {
                value.parts().map(AnyStringPart::Bytes).collect()
            }
            Self::FString(ExprFString { value, .. }) => value
                .parts()
                .map(|f_string_part| match f_string_part {
                    ast::FStringPart::Literal(string_literal) => {
                        AnyStringPart::String(string_literal)
                    }
                    ast::FStringPart::FString(f_string) => AnyStringPart::FString(f_string),
                })
                .collect(),
        }
    }
}

impl Ranged for AnyString<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::String(expr) => expr.range(),
            Self::Bytes(expr) => expr.range(),
            Self::FString(expr) => expr.range(),
        }
    }
}

impl<'a> From<&AnyString<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyString<'a>) -> Self {
        match value {
            AnyString::String(expr) => AnyNodeRef::ExprStringLiteral(expr),
            AnyString::Bytes(expr) => AnyNodeRef::ExprBytesLiteral(expr),
            AnyString::FString(expr) => AnyNodeRef::ExprFString(expr),
        }
    }
}

impl<'a> From<&AnyString<'a>> for ExpressionRef<'a> {
    fn from(value: &AnyString<'a>) -> Self {
        match value {
            AnyString::String(expr) => ExpressionRef::StringLiteral(expr),
            AnyString::Bytes(expr) => ExpressionRef::BytesLiteral(expr),
            AnyString::FString(expr) => ExpressionRef::FString(expr),
        }
    }
}

#[derive(Clone, Debug)]
enum AnyStringPart<'a> {
    String(&'a ast::StringLiteral),
    Bytes(&'a ast::BytesLiteral),
    FString(&'a ast::FString),
}

impl<'a> From<&AnyStringPart<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyStringPart<'a>) -> Self {
        match value {
            AnyStringPart::String(part) => AnyNodeRef::StringLiteral(part),
            AnyStringPart::Bytes(part) => AnyNodeRef::BytesLiteral(part),
            AnyStringPart::FString(part) => AnyNodeRef::FString(part),
        }
    }
}

impl Ranged for AnyStringPart<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::String(part) => part.range(),
            Self::Bytes(part) => part.range(),
            Self::FString(part) => part.range(),
        }
    }
}

pub(super) struct FormatString<'a> {
    string: &'a AnyString<'a>,
    layout: StringLayout,
}

#[derive(Default, Copy, Clone, Debug)]
pub enum StringLayout {
    #[default]
    Default,
    DocString,
    /// An implicit concatenated string in a binary like (e.g. `a + b` or `a < b`) expression.
    ///
    /// Formats the implicit concatenated string parts without the enclosing group because the group
    /// is added by the binary like formatting.
    ImplicitConcatenatedStringInBinaryLike,
}

impl<'a> FormatString<'a> {
    pub(super) fn new(string: &'a AnyString<'a>) -> Self {
        Self {
            string,
            layout: StringLayout::Default,
        }
    }

    pub(super) fn with_layout(mut self, layout: StringLayout) -> Self {
        self.layout = layout;
        self
    }
}

impl<'a> Format<PyFormatContext<'_>> for FormatString<'a> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let locator = f.context().locator();
        let result = match self.layout {
            StringLayout::Default => {
                if self.string.is_implicit_concatenated() {
                    in_parentheses_only_group(&FormatStringContinuation::new(self.string)).fmt(f)
                } else {
                    StringPart::from_source(self.string.range(), &locator)
                        .normalize(
                            self.string.quoting(&locator),
                            &locator,
                            f.options().quote_style(),
                        )
                        .fmt(f)
                }
            }
            StringLayout::DocString => {
                let string_part = StringPart::from_source(self.string.range(), &locator);
                let normalized =
                    string_part.normalize(Quoting::CanChange, &locator, f.options().quote_style());
                format_docstring(&normalized, f)
            }
            StringLayout::ImplicitConcatenatedStringInBinaryLike => {
                FormatStringContinuation::new(self.string).fmt(f)
            }
        };
        // TODO(dhruvmanila): With PEP 701, comments can be inside f-strings.
        // This is to mark all of those comments as formatted but we need to
        // figure out how to handle them. Note that this needs to be done only
        // after the f-string is formatted, so only for all the non-formatted
        // comments.
        if let AnyString::FString(fstring) = self.string {
            let comments = f.context().comments();
            fstring.value.elements().for_each(|value| {
                comments.mark_verbatim_node_comments_formatted(value.into());
            });
        }
        result
    }
}

struct FormatStringContinuation<'a> {
    string: &'a AnyString<'a>,
}

impl<'a> FormatStringContinuation<'a> {
    fn new(string: &'a AnyString<'a>) -> Self {
        Self { string }
    }
}

impl Format<PyFormatContext<'_>> for FormatStringContinuation<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let locator = f.context().locator();
        let quote_style = f.options().quote_style();

        let mut joiner = f.join_with(in_parentheses_only_soft_line_break_or_space());

        for part in self.string.parts() {
            let normalized = StringPart::from_source(part.range(), &locator).normalize(
                self.string.quoting(&locator),
                &locator,
                quote_style,
            );

            joiner.entry(&format_args![
                line_suffix_boundary(),
                leading_comments(comments.leading(&part)),
                normalized,
                trailing_comments(comments.trailing(&part))
            ]);
        }

        joiner.finish()
    }
}

#[derive(Debug)]
struct StringPart {
    /// The prefix.
    prefix: StringPrefix,

    /// The actual quotes of the string in the source
    quotes: StringQuotes,

    /// The range of the string's content (full range minus quotes and prefix)
    content_range: TextRange,
}

impl StringPart {
    fn from_source(range: TextRange, locator: &Locator) -> Self {
        let string_content = locator.slice(range);

        let prefix = StringPrefix::parse(string_content);
        let after_prefix = &string_content[usize::from(prefix.text_len())..];

        let quotes =
            StringQuotes::parse(after_prefix).expect("Didn't find string quotes after prefix");
        let relative_raw_content_range = TextRange::new(
            prefix.text_len() + quotes.text_len(),
            string_content.text_len() - quotes.text_len(),
        );
        let raw_content_range = relative_raw_content_range + range.start();

        Self {
            prefix,
            content_range: raw_content_range,
            quotes,
        }
    }

    /// Computes the strings preferred quotes and normalizes its content.
    fn normalize<'a>(
        self,
        quoting: Quoting,
        locator: &'a Locator,
        configured_style: QuoteStyle,
    ) -> NormalizedString<'a> {
        // Per PEP 8 and PEP 257, always prefer double quotes for docstrings and triple-quoted
        // strings. (We assume docstrings are always triple-quoted.)
        let preferred_style = if self.quotes.triple {
            QuoteStyle::Double
        } else {
            configured_style
        };

        let raw_content = locator.slice(self.content_range);

        let quotes = match quoting {
            Quoting::Preserve => self.quotes,
            Quoting::CanChange => {
                if self.prefix.is_raw_string() {
                    choose_quotes_raw(raw_content, self.quotes, preferred_style)
                } else {
                    choose_quotes(raw_content, self.quotes, preferred_style)
                }
            }
        };

        let normalized = normalize_string(locator.slice(self.content_range), quotes, self.prefix);

        NormalizedString {
            prefix: self.prefix,
            content_range: self.content_range,
            text: normalized,
            quotes,
        }
    }
}

#[derive(Debug)]
struct NormalizedString<'a> {
    prefix: StringPrefix,

    /// The quotes of the normalized string (preferred quotes)
    quotes: StringQuotes,

    /// The range of the string's content in the source (minus prefix and quotes).
    content_range: TextRange,

    /// The normalized text
    text: Cow<'a, str>,
}

impl Ranged for NormalizedString<'_> {
    fn range(&self) -> TextRange {
        self.content_range
    }
}

impl Format<PyFormatContext<'_>> for NormalizedString<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        write!(f, [self.prefix, self.quotes])?;
        match &self.text {
            Cow::Borrowed(_) => {
                source_text_slice(self.range()).fmt(f)?;
            }
            Cow::Owned(normalized) => {
                text(normalized, Some(self.start())).fmt(f)?;
            }
        }
        self.quotes.fmt(f)
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub(super) struct StringPrefix: u8 {
        const UNICODE   = 0b0000_0001;
        /// `r"test"`
        const RAW       = 0b0000_0010;
        /// `R"test"
        const RAW_UPPER = 0b0000_0100;
        const BYTE      = 0b0000_1000;
        const F_STRING  = 0b0001_0000;
    }
}

impl StringPrefix {
    pub(super) fn parse(input: &str) -> StringPrefix {
        let chars = input.chars();
        let mut prefix = StringPrefix::empty();

        for c in chars {
            let flag = match c {
                'u' | 'U' => StringPrefix::UNICODE,
                'f' | 'F' => StringPrefix::F_STRING,
                'b' | 'B' => StringPrefix::BYTE,
                'r' => StringPrefix::RAW,
                'R' => StringPrefix::RAW_UPPER,
                '\'' | '"' => break,
                c => {
                    unreachable!(
                        "Unexpected character '{c}' terminating the prefix of a string literal"
                    );
                }
            };

            prefix |= flag;
        }

        prefix
    }

    pub(super) const fn text_len(self) -> TextSize {
        TextSize::new(self.bits().count_ones())
    }

    pub(super) const fn is_raw_string(self) -> bool {
        self.contains(StringPrefix::RAW) || self.contains(StringPrefix::RAW_UPPER)
    }

    pub(super) const fn is_fstring(self) -> bool {
        self.contains(StringPrefix::F_STRING)
    }
}

impl Format<PyFormatContext<'_>> for StringPrefix {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        // Retain the casing for the raw prefix:
        // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        if self.contains(StringPrefix::RAW) {
            token("r").fmt(f)?;
        } else if self.contains(StringPrefix::RAW_UPPER) {
            token("R").fmt(f)?;
        }

        if self.contains(StringPrefix::BYTE) {
            token("b").fmt(f)?;
        }

        if self.contains(StringPrefix::F_STRING) {
            token("f").fmt(f)?;
        }

        // Remove the unicode prefix `u` if any because it is meaningless in Python 3+.

        Ok(())
    }
}

/// Choose the appropriate quote style for a raw string.
///
/// The preferred quote style is chosen unless the string contains unescaped quotes of the
/// preferred style. For example, `r"foo"` is chosen over `r'foo'` if the preferred quote
/// style is double quotes.
fn choose_quotes_raw(
    input: &str,
    quotes: StringQuotes,
    preferred_style: QuoteStyle,
) -> StringQuotes {
    let preferred_quote_char = preferred_style.as_char();
    let mut chars = input.chars().peekable();
    let contains_unescaped_configured_quotes = loop {
        match chars.next() {
            Some('\\') => {
                // Ignore escaped characters
                chars.next();
            }
            // `"` or `'`
            Some(c) if c == preferred_quote_char => {
                if !quotes.triple {
                    break true;
                }

                match chars.peek() {
                    // We can't turn `r'''\""'''` into `r"""\"""""`, this would confuse the parser
                    // about where the closing triple quotes start
                    None => break true,
                    Some(next) if *next == preferred_quote_char => {
                        // `""` or `''`
                        chars.next();

                        // We can't turn `r'''""'''` into `r""""""""`, nor can we have
                        // `"""` or `'''` respectively inside the string
                        if chars.peek().is_none() || chars.peek() == Some(&preferred_quote_char) {
                            break true;
                        }
                    }
                    _ => {}
                }
            }
            Some(_) => continue,
            None => break false,
        }
    };

    StringQuotes {
        triple: quotes.triple,
        style: if contains_unescaped_configured_quotes {
            quotes.style
        } else {
            preferred_style
        },
    }
}

/// Choose the appropriate quote style for a string.
///
/// For single quoted strings, the preferred quote style is used, unless the alternative quote style
/// would require fewer escapes.
///
/// For triple quoted strings, the preferred quote style is always used, unless the string contains
/// a triplet of the quote character (e.g., if double quotes are preferred, double quotes will be
/// used unless the string contains `"""`).
fn choose_quotes(input: &str, quotes: StringQuotes, preferred_style: QuoteStyle) -> StringQuotes {
    let style = if quotes.triple {
        // True if the string contains a triple quote sequence of the configured quote style.
        let mut uses_triple_quotes = false;
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            let preferred_quote_char = preferred_style.as_char();
            match c {
                '\\' => {
                    if matches!(chars.peek(), Some('"' | '\\')) {
                        chars.next();
                    }
                }
                // `"` or `'`
                c if c == preferred_quote_char => {
                    match chars.peek().copied() {
                        Some(c) if c == preferred_quote_char => {
                            // `""` or `''`
                            chars.next();

                            match chars.peek().copied() {
                                Some(c) if c == preferred_quote_char => {
                                    // `"""` or `'''`
                                    chars.next();
                                    uses_triple_quotes = true;
                                    break;
                                }
                                Some(_) => {}
                                None => {
                                    // Handle `''' ""'''`. At this point we have consumed both
                                    // double quotes, so on the next iteration the iterator is empty
                                    // and we'd miss the string ending with a preferred quote
                                    uses_triple_quotes = true;
                                    break;
                                }
                            }
                        }
                        Some(_) => {
                            // A single quote char, this is ok
                        }
                        None => {
                            // Trailing quote at the end of the comment
                            uses_triple_quotes = true;
                            break;
                        }
                    }
                }
                _ => continue,
            }
        }

        if uses_triple_quotes {
            // String contains a triple quote sequence of the configured quote style.
            // Keep the existing quote style.
            quotes.style
        } else {
            preferred_style
        }
    } else {
        let mut single_quotes = 0u32;
        let mut double_quotes = 0u32;

        for c in input.chars() {
            match c {
                '\'' => {
                    single_quotes += 1;
                }

                '"' => {
                    double_quotes += 1;
                }

                _ => continue,
            }
        }

        match preferred_style {
            QuoteStyle::Single => {
                if single_quotes > double_quotes {
                    QuoteStyle::Double
                } else {
                    QuoteStyle::Single
                }
            }
            QuoteStyle::Double => {
                if double_quotes > single_quotes {
                    QuoteStyle::Single
                } else {
                    QuoteStyle::Double
                }
            }
        }
    };

    StringQuotes {
        triple: quotes.triple,
        style,
    }
}

#[derive(Copy, Clone, Debug)]
pub(super) struct StringQuotes {
    triple: bool,
    style: QuoteStyle,
}

impl StringQuotes {
    pub(super) fn parse(input: &str) -> Option<StringQuotes> {
        let mut chars = input.chars();

        let quote_char = chars.next()?;
        let style = QuoteStyle::try_from(quote_char).ok()?;

        let triple = chars.next() == Some(quote_char) && chars.next() == Some(quote_char);

        Some(Self { triple, style })
    }

    pub(super) const fn is_triple(self) -> bool {
        self.triple
    }

    const fn text_len(self) -> TextSize {
        if self.triple {
            TextSize::new(3)
        } else {
            TextSize::new(1)
        }
    }
}

impl Format<PyFormatContext<'_>> for StringQuotes {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let quotes = match (self.style, self.triple) {
            (QuoteStyle::Single, false) => "'",
            (QuoteStyle::Single, true) => "'''",
            (QuoteStyle::Double, false) => "\"",
            (QuoteStyle::Double, true) => "\"\"\"",
        };

        token(quotes).fmt(f)
    }
}

/// Adds the necessary quote escapes and removes unnecessary escape sequences when quoting `input`
/// with the provided [`StringQuotes`] style.
///
/// Returns the normalized string and whether it contains new lines.
fn normalize_string(input: &str, quotes: StringQuotes, prefix: StringPrefix) -> Cow<str> {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let style = quotes.style;
    let preferred_quote = style.as_char();
    let opposite_quote = style.invert().as_char();

    let mut chars = input.char_indices().peekable();

    let is_raw = prefix.is_raw_string();
    let is_fstring = prefix.is_fstring();
    let mut formatted_value_nesting = 0u32;

    while let Some((index, c)) = chars.next() {
        if is_fstring && matches!(c, '{' | '}') {
            if chars.peek().copied().is_some_and(|(_, next)| next == c) {
                // Skip over the second character of the double braces
                chars.next();
            } else if c == '{' {
                formatted_value_nesting += 1;
            } else {
                // Safe to assume that `c == '}'` here because of the matched pattern above
                formatted_value_nesting = formatted_value_nesting.saturating_sub(1);
            }
            continue;
        }
        if c == '\r' {
            output.push_str(&input[last_index..index]);

            // Skip over the '\r' character, keep the `\n`
            if chars.peek().copied().is_some_and(|(_, next)| next == '\n') {
                chars.next();
            }
            // Replace the `\r` with a `\n`
            else {
                output.push('\n');
            }

            last_index = index + '\r'.len_utf8();
        } else if !quotes.triple && !is_raw {
            if c == '\\' {
                if let Some((_, next)) = chars.peek().copied() {
                    #[allow(clippy::if_same_then_else)]
                    if next == opposite_quote && formatted_value_nesting == 0 {
                        // Remove the escape by ending before the backslash and starting again with the quote
                        chars.next();
                        output.push_str(&input[last_index..index]);
                        last_index = index + '\\'.len_utf8();
                    } else if next == preferred_quote {
                        // Quote is already escaped, skip over it.
                        chars.next();
                    } else if next == '\\' {
                        // Skip over escaped backslashes
                        chars.next();
                    }
                }
            } else if c == preferred_quote && formatted_value_nesting == 0 {
                // Escape the quote
                output.push_str(&input[last_index..index]);
                output.push('\\');
                output.push(c);
                last_index = index + preferred_quote.len_utf8();
            }
        }
    }

    let normalized = if last_index == 0 {
        Cow::Borrowed(input)
    } else {
        output.push_str(&input[last_index..]);
        Cow::Owned(output)
    };

    normalized
}

/// For docstring indentation, black counts spaces as 1 and tabs by increasing the indentation up
/// to the next multiple of 8. This is effectively a port of
/// [`str.expandtabs`](https://docs.python.org/3/library/stdtypes.html#str.expandtabs),
/// which black [calls with the default tab width of 8](https://github.com/psf/black/blob/c36e468794f9256d5e922c399240d49782ba04f1/src/black/strings.py#L61).
fn indentation_length(line: &str) -> TextSize {
    let mut indentation = 0u32;
    for char in line.chars() {
        if char == '\t' {
            // Pad to the next multiple of tab_width
            indentation += 8 - (indentation.rem_euclid(8));
        } else if char.is_whitespace() {
            indentation += u32::from(char.text_len());
        } else {
            break;
        }
    }
    TextSize::new(indentation)
}

/// Format a docstring by trimming whitespace and adjusting the indentation.
///
/// Summary of changes we make:
/// * Normalize the string like all other strings
/// * Ignore docstring that have an escaped newline
/// * Trim all trailing whitespace, except for a chaperone space that avoids quotes or backslashes
///   in the last line.
/// * Trim leading whitespace on the first line, again except for a chaperone space
/// * If there is only content in the first line and after that only whitespace, collapse the
///   docstring into one line
/// * Adjust the indentation (see below)
///
/// # Docstring indentation
///
/// Unlike any other string, like black we change the indentation of docstring lines.
///
/// We want to preserve the indentation inside the docstring relative to the suite statement/block
/// indent that the docstring statement is in, but also want to apply the change of the outer
/// indentation in the docstring, e.g.
/// ```python
/// def sparkle_sky():
///   """Make a pretty sparkly sky.
///   *       * ✨        *.    .
///      *       *      ✨      .
///      .  *      . ✨    * .  .
///   """
/// ```
/// should become
/// ```python
/// def sparkle_sky():
///     """Make a pretty sparkly sky.
///     *       * ✨        *.    .
///        *       *      ✨      .
///        .  *      . ✨    * .  .
///     """
/// ```
/// We can't compute the full indentation here since we don't know what the block indent of
/// the doc comment will be yet and which we can only have added by formatting each line
/// separately with a hard line break. This means we need to strip shared indentation from
/// docstring while preserving the in-docstring bigger-than-suite-statement indentation. Example:
/// ```python
/// def f():
///  """first line
///  line a
///     line b
///  """
/// ```
/// The docstring indentation is 2, the block indents will change this to 4 (but we can't
/// determine this at this point). The indentation of line a is 2, so we trim `  line a`
/// to `line a`. For line b it's 5, so we trim it to `line b` and pad with 5-2=3 spaces to
/// `   line b`. The closing quotes, being on their own line, are stripped get only the
/// default indentation. Fully formatted:
/// ```python
/// def f():
///    """first line
///    line a
///       line b
///    """
/// ```
///
/// Tabs are counted by padding them to the next multiple of 8 according to
/// [`str.expandtabs`](https://docs.python.org/3/library/stdtypes.html#str.expandtabs). When
/// we see indentation that contains a tab or any other none ascii-space whitespace we rewrite the
/// string.
///
/// Additionally, if any line in the docstring has less indentation than the docstring
/// (effectively a negative indentation wrt. to the current level), we pad all lines to the
/// level of the docstring with spaces.
/// ```python
/// def f():
///    """first line
/// line a
///    line b
///      line c
///    """
/// ```
/// Here line a is 3 columns negatively indented, so we pad all lines by an extra 3 spaces:
/// ```python
/// def f():
///    """first line
///    line a
///       line b
///         line c
///    """
/// ```
fn format_docstring(normalized: &NormalizedString, f: &mut PyFormatter) -> FormatResult<()> {
    let docstring = &normalized.text;

    // Black doesn't change the indentation of docstrings that contain an escaped newline
    if docstring.contains("\\\n") {
        return normalized.fmt(f);
    }

    // is_borrowed is unstable :/
    let already_normalized = matches!(docstring, Cow::Borrowed(_));

    let mut lines = docstring.lines().peekable();

    // Start the string
    write!(
        f,
        [
            normalized.prefix,
            normalized.quotes,
            source_position(normalized.start()),
        ]
    )?;
    // We track where in the source docstring we are (in source code byte offsets)
    let mut offset = normalized.start();

    // The first line directly after the opening quotes has different rules than the rest, mainly
    // that we remove all leading whitespace as there's no indentation
    let first = lines.next().unwrap_or_default();
    // Black trims whitespace using [`str.strip()`](https://docs.python.org/3/library/stdtypes.html#str.strip)
    // https://github.com/psf/black/blob/b4dca26c7d93f930bbd5a7b552807370b60d4298/src/black/strings.py#L77-L85
    // So we use the unicode whitespace definition through `trim_{start,end}` instead of the python
    // tokenizer whitespace definition in `trim_whitespace_{start,end}`.
    let trim_end = first.trim_end();
    let trim_both = trim_end.trim_start();

    // Edge case: The first line is `""" "content`, so we need to insert chaperone space that keep
    // inner quotes and closing quotes from getting to close to avoid `""""content`
    if trim_both.starts_with(normalized.quotes.style.as_char()) {
        space().fmt(f)?;
    }

    if !trim_end.is_empty() {
        // For the first line of the docstring we strip the leading and trailing whitespace, e.g.
        // `"""   content   ` to `"""content`
        let leading_whitespace = trim_end.text_len() - trim_both.text_len();
        let trimmed_line_range =
            TextRange::at(offset, trim_end.text_len()).add_start(leading_whitespace);
        if already_normalized {
            source_text_slice(trimmed_line_range).fmt(f)?;
        } else {
            text(trim_both, Some(trimmed_line_range.start())).fmt(f)?;
        }
    }
    offset += first.text_len();

    // Check if we have a single line (or empty) docstring
    if docstring[first.len()..].trim().is_empty() {
        // For `"""\n"""` or other whitespace between the quotes, black keeps a single whitespace,
        // but `""""""` doesn't get one inserted.
        if needs_chaperone_space(normalized, trim_end)
            || (trim_end.is_empty() && !docstring.is_empty())
        {
            space().fmt(f)?;
        }
        normalized.quotes.fmt(f)?;
        return Ok(());
    }

    hard_line_break().fmt(f)?;
    // We know that the normalized string has \n line endings
    offset += "\n".text_len();

    // If some line of the docstring is less indented than the function body, we pad all lines to
    // align it with the docstring statement. Conversely, if all lines are over-indented, we strip
    // the extra indentation. We call this stripped indentation since it's relative to the block
    // indent printer-made indentation.
    let stripped_indentation = lines
        .clone()
        // We don't want to count whitespace-only lines as miss-indented
        .filter(|line| !line.trim().is_empty())
        .map(indentation_length)
        .min()
        .unwrap_or_default();

    while let Some(line) = lines.next() {
        let is_last = lines.peek().is_none();
        format_docstring_line(
            line,
            is_last,
            offset,
            stripped_indentation,
            already_normalized,
            f,
        )?;
        // We know that the normalized string has \n line endings
        offset += line.text_len() + "\n".text_len();
    }

    // Same special case in the last line as for the first line
    let trim_end = docstring
        .as_ref()
        .trim_end_matches(|c: char| c.is_whitespace() && c != '\n');
    if needs_chaperone_space(normalized, trim_end) {
        space().fmt(f)?;
    }

    write!(f, [source_position(normalized.end()), normalized.quotes])
}

/// If the last line of the docstring is `content" """` or `content\ """`, we need a chaperone space
/// that avoids `content""""` and `content\"""`. This does only applies to un-escaped backslashes,
/// so `content\\ """` doesn't need a space while `content\\\ """` does.
fn needs_chaperone_space(normalized: &NormalizedString, trim_end: &str) -> bool {
    trim_end.ends_with(normalized.quotes.style.as_char())
        || trim_end.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

/// Format a docstring line that is not the first line
fn format_docstring_line(
    line: &str,
    is_last: bool,
    offset: TextSize,
    stripped_indentation_length: TextSize,
    already_normalized: bool,
    f: &mut PyFormatter,
) -> FormatResult<()> {
    let trim_end = line.trim_end();
    if trim_end.is_empty() {
        return if is_last {
            // If the doc string ends with `    """`, the last line is `    `, but we don't want to
            // insert an empty line (but close the docstring)
            Ok(())
        } else {
            empty_line().fmt(f)
        };
    }

    let tab_or_non_ascii_space = trim_end
        .chars()
        .take_while(|c| c.is_whitespace())
        .any(|c| c != ' ');

    if tab_or_non_ascii_space {
        // We strip the indentation that is shared with the docstring statement, unless a line
        // was indented less than the docstring statement, in which we strip only this much
        // indentation to implicitly pad all lines by the difference, or all lines were
        // overindented, in which case we strip the additional whitespace (see example in
        // [`format_docstring`] doc comment). We then prepend the in-docstring indentation to the
        // string.
        let indent_len = indentation_length(trim_end) - stripped_indentation_length;
        let in_docstring_indent = " ".repeat(usize::from(indent_len)) + trim_end.trim_start();
        text(&in_docstring_indent, Some(offset)).fmt(f)?;
    } else {
        // Take the string with the trailing whitespace removed, then also skip the leading
        // whitespace
        let trimmed_line_range =
            TextRange::at(offset, trim_end.text_len()).add_start(stripped_indentation_length);
        if already_normalized {
            source_text_slice(trimmed_line_range).fmt(f)?;
        } else {
            // All indents are ascii spaces, so the slicing is correct
            text(
                &trim_end[usize::from(stripped_indentation_length)..],
                Some(trimmed_line_range.start()),
            )
            .fmt(f)?;
        }
    }

    // We handled the case that the closing quotes are on their own line above (the last line is
    // empty except for whitespace). If they are on the same line as content, we don't insert a line
    // break.
    if !is_last {
        hard_line_break().fmt(f)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::expression::string::indentation_length;
    use ruff_text_size::TextSize;

    #[test]
    fn test_indentation_like_black() {
        assert_eq!(indentation_length("\t \t  \t"), TextSize::new(24));
        assert_eq!(indentation_length("\t        \t"), TextSize::new(24));
        assert_eq!(indentation_length("\t\t\t"), TextSize::new(24));
        assert_eq!(indentation_length("    "), TextSize::new(4));
    }
}
