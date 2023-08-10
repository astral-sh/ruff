use std::borrow::Cow;

use bitflags::bitflags;

use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::str::is_implicit_concatenation;
use ruff_python_ast::{self as ast, ExprConstant, ExprFString, Ranged};
use ruff_python_parser::lexer::{lex_starts_at, LexicalError, LexicalErrorType};
use ruff_python_parser::{Mode, Tok};
use ruff_source_file::Locator;
use ruff_text_size::{TextLen, TextRange, TextSize};

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

pub(super) enum AnyString<'a> {
    Constant(&'a ExprConstant),
    FString(&'a ExprFString),
}

impl<'a> AnyString<'a> {
    fn quoting(&self, locator: &Locator) -> Quoting {
        match self {
            Self::Constant(_) => Quoting::CanChange,
            Self::FString(f_string) => {
                if f_string.values.iter().any(|value| match value {
                    Expr::FormattedValue(ast::ExprFormattedValue { range, .. }) => {
                        let string_content = locator.slice(*range);
                        string_content.contains(['"', '\''])
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
}

impl Ranged for AnyString<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Constant(expr) => expr.range(),
            Self::FString(expr) => expr.range(),
        }
    }
}

impl<'a> From<&AnyString<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyString<'a>) -> Self {
        match value {
            AnyString::Constant(expr) => AnyNodeRef::ExprConstant(expr),
            AnyString::FString(expr) => AnyNodeRef::ExprFString(expr),
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
    ImplicitConcatenatedBinaryLeftSide,
}

impl<'a> FormatString<'a> {
    pub(super) fn new(string: &'a AnyString) -> Self {
        if let AnyString::Constant(constant) = string {
            debug_assert!(constant.value.is_str() || constant.value.is_bytes());
        }
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
        match self.layout {
            StringLayout::Default => {
                let string_range = self.string.range();
                let string_content = f.context().locator().slice(string_range);

                if is_implicit_concatenation(string_content) {
                    in_parentheses_only_group(&FormatStringContinuation::new(self.string)).fmt(f)
                } else {
                    FormatStringPart::new(
                        string_range,
                        self.string.quoting(&f.context().locator()),
                        &f.context().locator(),
                        f.options().quote_style(),
                    )
                    .fmt(f)
                }
            }
            StringLayout::DocString => format_docstring(self.string.range(), f),
            StringLayout::ImplicitConcatenatedBinaryLeftSide => {
                FormatStringContinuation::new(self.string).fmt(f)
            }
        }
    }
}

struct FormatStringContinuation<'a> {
    string: &'a AnyString<'a>,
}

impl<'a> FormatStringContinuation<'a> {
    fn new(string: &'a AnyString<'a>) -> Self {
        if let AnyString::Constant(constant) = string {
            debug_assert!(constant.value.is_str() || constant.value.is_bytes());
        }
        Self { string }
    }
}

impl Format<PyFormatContext<'_>> for FormatStringContinuation<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let locator = f.context().locator();
        let quote_style = f.options().quote_style();
        let mut dangling_comments = comments.dangling_comments(self.string);

        let string_range = self.string.range();
        let string_content = locator.slice(string_range);

        // The AST parses implicit concatenation as a single string.
        // Call into the lexer to extract the individual chunks and format each string on its own.
        // This code does not yet implement the automatic joining of strings that fit on the same line
        // because this is a black preview style.
        let lexer = lex_starts_at(string_content, Mode::Expression, string_range.start());

        let mut joiner = f.join_with(in_parentheses_only_soft_line_break_or_space());

        for token in lexer {
            let (token, token_range) = match token {
                Ok(spanned) => spanned,
                Err(LexicalError {
                    error: LexicalErrorType::IndentationError,
                    ..
                }) => {
                    // This can happen if the string continuation appears anywhere inside of a parenthesized expression
                    // because the lexer doesn't know about the parentheses. For example, the following snipped triggers an Indentation error
                    // ```python
                    // {
                    //     "key": (
                    //         [],
                    //         'a'
                    //             'b'
                    //          'c'
                    //     )
                    // }
                    // ```
                    // Ignoring the error here is *safe* because we know that the program once parsed to a valid AST.
                    continue;
                }
                Err(_) => {
                    return Err(FormatError::syntax_error(
                        "Unexpected lexer error in string formatting",
                    ));
                }
            };

            match token {
                Tok::String { .. } => {
                    // ```python
                    // (
                    //      "a"
                    //      # leading
                    //      "the comment above"
                    // )
                    // ```
                    let leading_comments_end = dangling_comments
                        .partition_point(|comment| comment.slice().start() <= token_range.start());

                    let (leading_part_comments, rest) =
                        dangling_comments.split_at(leading_comments_end);

                    // ```python
                    // (
                    //      "a" # trailing comment
                    //      "the comment above"
                    // )
                    // ```
                    let trailing_comments_end = rest.partition_point(|comment| {
                        comment.line_position().is_end_of_line()
                            && !locator.contains_line_break(TextRange::new(
                                token_range.end(),
                                comment.slice().start(),
                            ))
                    });

                    let (trailing_part_comments, rest) = rest.split_at(trailing_comments_end);

                    joiner.entry(&format_args![
                        line_suffix_boundary(),
                        leading_comments(leading_part_comments),
                        FormatStringPart::new(
                            token_range,
                            self.string.quoting(&locator),
                            &locator,
                            quote_style,
                        ),
                        trailing_comments(trailing_part_comments)
                    ]);

                    dangling_comments = rest;
                }
                Tok::Comment(_)
                | Tok::NonLogicalNewline
                | Tok::Newline
                | Tok::Indent
                | Tok::Dedent => continue,
                token => unreachable!("Unexpected token {token:?}"),
            }
        }

        debug_assert!(dangling_comments.is_empty());

        joiner.finish()
    }
}

struct FormatStringPart {
    prefix: StringPrefix,
    preferred_quotes: StringQuotes,
    range: TextRange,
    is_raw_string: bool,
}

impl FormatStringPart {
    fn new(range: TextRange, quoting: Quoting, locator: &Locator, quote_style: QuoteStyle) -> Self {
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

        let raw_content = &string_content[relative_raw_content_range];
        let is_raw_string = prefix.is_raw_string();
        let preferred_quotes = match quoting {
            Quoting::Preserve => quotes,
            Quoting::CanChange => {
                if is_raw_string {
                    preferred_quotes_raw(raw_content, quotes, quote_style)
                } else {
                    preferred_quotes(raw_content, quotes, quote_style)
                }
            }
        };

        Self {
            prefix,
            range: raw_content_range,
            preferred_quotes,
            is_raw_string,
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatStringPart {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let (normalized, contains_newlines) = normalize_string(
            f.context().locator().slice(self.range),
            self.preferred_quotes,
            self.is_raw_string,
        );

        write!(f, [self.prefix, self.preferred_quotes])?;
        match normalized {
            Cow::Borrowed(_) => {
                source_text_slice(self.range, contains_newlines).fmt(f)?;
            }
            Cow::Owned(normalized) => {
                dynamic_text(&normalized, Some(self.range.start())).fmt(f)?;
            }
        }
        self.preferred_quotes.fmt(f)
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
        matches!(self, StringPrefix::RAW | StringPrefix::RAW_UPPER)
    }
}

impl Format<PyFormatContext<'_>> for StringPrefix {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        // Retain the casing for the raw prefix:
        // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        if self.contains(StringPrefix::RAW) {
            text("r").fmt(f)?;
        } else if self.contains(StringPrefix::RAW_UPPER) {
            text("R").fmt(f)?;
        }

        if self.contains(StringPrefix::BYTE) {
            text("b").fmt(f)?;
        }

        if self.contains(StringPrefix::F_STRING) {
            text("f").fmt(f)?;
        }

        // Remove the unicode prefix `u` if any because it is meaningless in Python 3+.

        Ok(())
    }
}

/// Detects the preferred quotes for raw string `input`.
/// The configured quote style is preferred unless `input` contains unescaped quotes of the
/// configured style. For example, `r"foo"` is preferred over `r'foo'` if the configured
/// quote style is double quotes.
fn preferred_quotes_raw(
    input: &str,
    quotes: StringQuotes,
    configured_style: QuoteStyle,
) -> StringQuotes {
    let configured_quote_char = configured_style.as_char();
    let mut chars = input.chars().peekable();
    let contains_unescaped_configured_quotes = loop {
        match chars.next() {
            Some('\\') => {
                // Ignore escaped characters
                chars.next();
            }
            // `"` or `'`
            Some(c) if c == configured_quote_char => {
                if !quotes.triple {
                    break true;
                }

                match chars.peek() {
                    // We can't turn `r'''\""'''` into `r"""\"""""`, this would confuse the parser
                    // about where the closing triple quotes start
                    None => break true,
                    Some(next) if *next == configured_quote_char => {
                        // `""` or `''`
                        chars.next();

                        // We can't turn `r'''""'''` into `r""""""""`, nor can we have
                        // `"""` or `'''` respectively inside the string
                        if chars.peek().is_none() || chars.peek() == Some(&configured_quote_char) {
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
            configured_style
        },
    }
}

/// Detects the preferred quotes for `input`.
/// * single quoted strings: The preferred quote style is the one that requires less escape sequences.
/// * triple quoted strings: Use double quotes except the string contains a sequence of `"""`.
fn preferred_quotes(
    input: &str,
    quotes: StringQuotes,
    configured_style: QuoteStyle,
) -> StringQuotes {
    let preferred_style = if quotes.triple {
        // True if the string contains a triple quote sequence of the configured quote style.
        let mut uses_triple_quotes = false;
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            let configured_quote_char = configured_style.as_char();
            match c {
                '\\' => {
                    if matches!(chars.peek(), Some('"' | '\\')) {
                        chars.next();
                    }
                }
                // `"` or `'`
                c if c == configured_quote_char => {
                    match chars.peek().copied() {
                        Some(c) if c == configured_quote_char => {
                            // `""` or `''`
                            chars.next();

                            if chars.peek().copied() == Some(configured_quote_char) {
                                // `"""` or `'''`
                                chars.next();
                                uses_triple_quotes = true;
                            }
                        }
                        Some(_) => {
                            // A single quote char, this is ok
                        }
                        None => {
                            // Trailing quote at the end of the comment
                            uses_triple_quotes = true;
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
            configured_style
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

        match configured_style {
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
        style: preferred_style,
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

        text(quotes).fmt(f)
    }
}

/// Adds the necessary quote escapes and removes unnecessary escape sequences when quoting `input`
/// with the provided `style`.
///
/// Returns the normalized string and whether it contains new lines.
fn normalize_string(
    input: &str,
    quotes: StringQuotes,
    is_raw: bool,
) -> (Cow<str>, ContainsNewlines) {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let mut newlines = ContainsNewlines::No;

    let style = quotes.style;
    let preferred_quote = style.as_char();
    let opposite_quote = style.invert().as_char();

    let mut chars = input.char_indices();

    while let Some((index, c)) = chars.next() {
        if c == '\r' {
            output.push_str(&input[last_index..index]);

            // Skip over the '\r' character, keep the `\n`
            if input.as_bytes().get(index + 1).copied() == Some(b'\n') {
                chars.next();
            }
            // Replace the `\r` with a `\n`
            else {
                output.push('\n');
            }

            last_index = index + '\r'.len_utf8();
            newlines = ContainsNewlines::Yes;
        } else if c == '\n' {
            newlines = ContainsNewlines::Yes;
        } else if !quotes.triple && !is_raw {
            if c == '\\' {
                if let Some(next) = input.as_bytes().get(index + 1).copied().map(char::from) {
                    #[allow(clippy::if_same_then_else)]
                    if next == opposite_quote {
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
            } else if c == preferred_quote {
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

    (normalized, newlines)
}

/// For docstring indentation, black counts spaces as 1 and tabs by increasing the indentation up
/// to the next multiple of 8. This is effectively a port of `str.expandtabs`, which black calls.
fn count_indentation_like_black(line: &str) -> TextSize {
    let tab_width: u32 = 8;
    let mut indentation = TextSize::default();
    for char in line.chars() {
        if char == '\t' {
            // Pad to the next multiple of tab_width
            indentation += TextSize::from(tab_width - (indentation.to_u32().rem_euclid(tab_width)));
        } else if char.is_whitespace() {
            indentation += char.text_len();
        } else {
            return indentation;
        }
    }
    indentation
}

/// Format a docstring by trimming whitespace and adjusting the indentation.
///
/// We trim all trailing whitespace, except for a chaperone space to avoid quotes or backslashes
/// in the
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
/// Tabs are counted by padding them to the next multiple of 8 according to `str.expandtabs`. When
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
fn format_docstring(
    // The start of the string prefix to after the closing quotes
    outer_range: TextRange,
    f: &mut PyFormatter,
) -> FormatResult<()> {
    let locator = f.context().locator();
    let string_part = FormatStringPart::new(
        outer_range,
        // It's not an f-string
        Quoting::CanChange,
        &locator,
        f.options().quote_style(),
    );

    // Black doesn't change the indentation of docstrings that contain an escaped newline
    if locator.slice(outer_range).contains("\\\n") {
        return string_part.fmt(f);
    }

    let (normalized, _) = normalize_string(
        locator.slice(string_part.range),
        string_part.preferred_quotes,
        string_part.is_raw_string,
    );
    // is_borrowed is unstable :/
    let already_normalized = matches!(normalized, Cow::Borrowed(_));

    let mut lines = normalized.lines().peekable();

    // Start the string
    write!(f, [string_part.prefix, string_part.preferred_quotes])?;
    // We track where in the source docstring we are (in source code byte offsets)
    let mut offset = string_part.range.start();

    // The first line directly after the opening quotes has different rules than the rest, mainly
    // that we remove all leading whitespace as there's no indentation
    let first = lines.next().unwrap_or_default();
    let trim_end = first.trim_end();

    // Edge case: The first line is `""" "content`, so we need to insert chaperone whitespace to
    // avoid `""""content`
    if trim_end
        .trim_start()
        .starts_with(string_part.preferred_quotes.style.as_char())
    {
        space().fmt(f)?;
    }

    if !trim_end.is_empty() {
        // For the first line of the docstring we strip the leading and trailing whitespace, e.g.
        // `"""   content   ` to `"""content`
        let leading_whitespace = trim_end.text_len() - trim_end.trim_start().text_len();
        let trimmed_line_range =
            TextRange::at(offset, trim_end.text_len()).add_start(leading_whitespace);
        if already_normalized {
            source_text_slice(trimmed_line_range, ContainsNewlines::No).fmt(f)?;
        } else {
            dynamic_text(trim_end.trim_start(), Some(trimmed_line_range.start())).fmt(f)?;
        }
    }
    offset += first.text_len();

    // Check if we have a single line (or empty) docstring
    if normalized[first.len()..].trim().is_empty() {
        // * The last line is `content" """` so we need a chaperone whitespace to avoid
        //   `content""""`.
        // * The last line is `content\ """` so we need a chaperone whitespace to avoid
        //   `content\"""`. Note that `content\\ """` doesn't need one while `content\\\ """` does.
        // * For `"""\n"""` or other whitespace between the quotes, black keeps a single whitespace,
        //   but `""""""` doesn't get one inserted.
        let needs_space = trim_end.ends_with(string_part.preferred_quotes.style.as_char())
            || trim_end.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
            || (trim_end.is_empty() && !normalized.is_empty());
        if needs_space {
            space().fmt(f)?;
        }
        string_part.preferred_quotes.fmt(f)?;
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
        .map(count_indentation_like_black)
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
    let trim_end = normalized.as_ref().trim_end();
    if trim_end.ends_with(string_part.preferred_quotes.style.as_char())
        || trim_end.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
    {
        space().fmt(f)?;
    }

    write!(f, [string_part.preferred_quotes])
}

/// Format a docstring line that is not the first line
fn format_docstring_line(
    line: &str,
    is_last: bool,
    offset: TextSize,
    stripped_indentation: TextSize,
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
        let indent_len = count_indentation_like_black(trim_end) - stripped_indentation;
        let in_docstring_indent = " ".repeat(indent_len.to_usize()) + trim_end.trim_start();
        dynamic_text(&in_docstring_indent, Some(offset)).fmt(f)?;
    } else {
        // Take the string with the trailing whitespace removed, then also skip the leading
        // whitespace
        let trimmed_line_range =
            TextRange::at(offset, trim_end.text_len()).add_start(stripped_indentation);
        if already_normalized {
            source_text_slice(trimmed_line_range, ContainsNewlines::No).fmt(f)?;
        } else {
            // All indents are ascii spaces, so the slicing is correct
            dynamic_text(
                &trim_end[stripped_indentation.to_usize()..],
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
    use crate::expression::string::count_indentation_like_black;

    #[test]
    fn test_indentation_like_black() {
        assert_eq!(count_indentation_like_black("\t \t  \t").to_u32(), 24);
        assert_eq!(count_indentation_like_black("\t        \t").to_u32(), 24);
        assert_eq!(count_indentation_like_black("\t\t\t").to_u32(), 24);
        assert_eq!(count_indentation_like_black("    ").to_u32(), 4);
    }
}
