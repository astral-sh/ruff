use itertools::Itertools;
use std::cmp::Ordering;
use std::num::NonZeroU32;
use std::slice::Iter;

use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Stylist;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::lexer::LexicalError;
use ruff_python_parser::Tok;
use ruff_python_parser::TokenKind;
use ruff_source_file::{Locator, UniversalNewlines};
use ruff_text_size::TextRange;
use ruff_text_size::TextSize;

use crate::checkers::logical_lines::expand_indent;
use crate::line_width::IndentWidth;
use ruff_python_trivia::PythonWhitespace;

/// Number of blank lines around top level classes and functions.
const BLANK_LINES_TOP_LEVEL: u32 = 2;
/// Number of blank lines around methods and nested classes and functions.
const BLANK_LINES_METHOD_LEVEL: u32 = 1;

/// ## What it does
/// Checks for missing blank lines between methods of a class.
///
/// ## Why is this bad?
/// PEP 8 recommends exactly one blank line between methods of a class.
///
/// ## Example
/// ```python
/// class MyClass(object):
///     def func1():
///         pass
///     def func2():
///         pass
/// ```
///
/// Use instead:
/// ```python
/// class MyClass(object):
///     def func1():
///         pass
///
///     def func2():
///         pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E301.html)
#[violation]
pub struct BlankLineBetweenMethods;

impl AlwaysFixableViolation for BlankLineBetweenMethods {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected {BLANK_LINES_METHOD_LEVEL:?} blank line, found 0")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line".to_string()
    }
}

/// ## What it does
/// Checks for missing blank lines between top level functions and classes.
///
/// ## Why is this bad?
/// PEP 8 recommends exactly two blank lines between top level functions and classes.
///
/// ## Example
/// ```python
/// def func1():
///     pass
/// def func2():
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def func1():
///     pass
///
///
/// def func2():
///     pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E302.html)
#[violation]
pub struct BlankLinesTopLevel {
    actual_blank_lines: u32,
}

impl AlwaysFixableViolation for BlankLinesTopLevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesTopLevel {
            actual_blank_lines: nb_blank_lines,
        } = self;

        format!("Expected {BLANK_LINES_TOP_LEVEL:?} blank lines, found {nb_blank_lines}")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for extraneous blank lines.
///
/// ## Why is this bad?
/// PEP 8 recommends using blank lines as follows:
/// - No more than two blank lines between top-level statements.
/// - No more than one blank line between non-top-level statements.
///
/// ## Example
/// ```python
/// def func1():
///     pass
///
///
///
/// def func2():
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def func1():
///     pass
///
///
/// def func2():
///     pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E303.html)
#[violation]
pub struct TooManyBlankLines {
    actual_blank_lines: u32,
}

impl AlwaysFixableViolation for TooManyBlankLines {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBlankLines {
            actual_blank_lines: nb_blank_lines,
        } = self;
        format!("Too many blank lines ({nb_blank_lines})")
    }

    fn fix_title(&self) -> String {
        "Remove extraneous blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for extraneous blank line(s) after function decorators.
///
/// ## Why is this bad?
/// There should be no blank lines between a decorator and the object it is decorating.
///
/// ## Example
/// ```python
/// class User(object):
///
///     @property
///
///     def name(self):
///         pass
/// ```
///
/// Use instead:
/// ```python
/// class User(object):
///
///     @property
///     def name(self):
///         pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E304.html)
#[violation]
pub struct BlankLineAfterDecorator {
    actual_blank_lines: u32,
}

impl AlwaysFixableViolation for BlankLineAfterDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Blank lines found after function decorator ({lines})",
            lines = self.actual_blank_lines
        )
    }

    fn fix_title(&self) -> String {
        "Remove extraneous blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for missing blank lines after the end of function or class.
///
/// ## Why is this bad?
/// PEP 8 recommends using blank lines as following:
/// - Two blank lines are expected between functions and classes
/// - One blank line is expected between methods of a class.
///
/// ## Example
/// ```python
/// class User(object):
///     pass
/// user = User()
/// ```
///
/// Use instead:
/// ```python
/// class User(object):
///     pass
///
///
/// user = User()
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E305.html)
#[violation]
pub struct BlankLinesAfterFunctionOrClass {
    actual_blank_lines: u32,
}

impl AlwaysFixableViolation for BlankLinesAfterFunctionOrClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesAfterFunctionOrClass {
            actual_blank_lines: blank_lines,
        } = self;
        format!("Expected 2 blank lines after class or function definition, found ({blank_lines})")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for 1 blank line between nested function or class definitions.
///
/// ## Why is this bad?
/// PEP 8 recommends using blank lines as following:
/// - Two blank lines are expected between functions and classes
/// - One blank line is expected between methods of a class.
///
/// ## Example
/// ```python
/// def outer():
///     def inner():
///         pass
///     def inner2():
///         pass
/// ```
///
/// Use instead:
/// ```python
/// def outer():
///     def inner():
///         pass
///
///     def inner2():
///         pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E306.html)
#[violation]
pub struct BlankLinesBeforeNestedDefinition;

impl AlwaysFixableViolation for BlankLinesBeforeNestedDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected 1 blank line before a nested definition, found 0")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line".to_string()
    }
}

#[derive(Debug)]
struct LogicalLineInfo {
    kind: LogicalLineKind,
    first_token_range: TextRange,

    // The token's kind right before the newline ending the logical line.
    last_token: TokenKind,

    // The end of the logical line including the newline.
    logical_line_end: TextSize,

    // `true` if this is not a blank but only consists of a comment.
    is_comment_only: bool,

    /// `true` if the line is a string only (including trivia tokens) line, which is a docstring if coming right after a class/function definition.
    is_docstring: bool,

    /// The indentation length in columns. See [`expand_indent`] for the computation of the indent.
    indent_length: usize,

    /// The number of blank lines preceding the current line.
    blank_lines: BlankLines,

    /// The maximum number of consecutive blank lines between the current line
    /// and the previous non-comment logical line.
    /// One of its main uses is to allow a comments to directly precede or follow a class/function definition.
    /// As such, `preceding_blank_lines` is used for rules that cannot trigger on comments (all rules except E303),
    /// and `blank_lines` is used for the rule that can trigger on comments (E303).
    preceding_blank_lines: BlankLines,
}

/// Iterator that processes tokens until a full logical line (or comment line) is "built".
/// It then returns characteristics of that logical line (see `LogicalLineInfo`).
struct LinePreprocessor<'a> {
    tokens: Iter<'a, Result<(Tok, TextRange), LexicalError>>,
    locator: &'a Locator<'a>,
    indent_width: IndentWidth,
    /// The start position of the next logical line.
    line_start: TextSize,
    /// Maximum number of consecutive blank lines between the current line and the previous non-comment logical line.
    /// One of its main uses is to allow a comment to directly precede a class/function definition.
    max_preceding_blank_lines: BlankLines,
}

impl<'a> LinePreprocessor<'a> {
    fn new(
        tokens: &'a [LexResult],
        locator: &'a Locator,
        indent_width: IndentWidth,
    ) -> LinePreprocessor<'a> {
        LinePreprocessor {
            tokens: tokens.iter(),
            locator,
            line_start: TextSize::new(0),
            max_preceding_blank_lines: BlankLines::Zero,
            indent_width,
        }
    }
}

impl<'a> Iterator for LinePreprocessor<'a> {
    type Item = LogicalLineInfo;

    fn next(&mut self) -> Option<LogicalLineInfo> {
        let mut line_is_comment_only = true;
        let mut is_docstring = false;
        // Number of consecutive blank lines directly preceding this logical line.
        let mut blank_lines = BlankLines::Zero;
        let mut first_logical_line_token: Option<(LogicalLineKind, TextRange)> = None;
        let mut last_token: TokenKind = TokenKind::EndOfFile;
        let mut parens = 0u32;

        while let Some(result) = self.tokens.next() {
            let Ok((token, range)) = result else {
                continue;
            };

            if matches!(token, Tok::Indent | Tok::Dedent) {
                continue;
            }

            let token_kind = TokenKind::from_token(token);

            let (logical_line_kind, first_token_range) = if let Some(first_token_range) =
                first_logical_line_token
            {
                first_token_range
            }
            // At the start of the line...
            else {
                // An empty line
                if token_kind == TokenKind::NonLogicalNewline {
                    blank_lines.add(*range);

                    self.line_start = range.end();

                    continue;
                }

                is_docstring = token_kind == TokenKind::String;

                let logical_line_kind = match token_kind {
                    TokenKind::Class => LogicalLineKind::Class,
                    TokenKind::Comment => LogicalLineKind::Comment,
                    TokenKind::At => LogicalLineKind::Decorator,
                    TokenKind::Def => LogicalLineKind::Function,
                    // Lookahead to distinguish `async def` from `async with`.
                    TokenKind::Async
                        if matches!(self.tokens.as_slice().first(), Some(Ok((Tok::Def, _)))) =>
                    {
                        LogicalLineKind::Function
                    }
                    _ => LogicalLineKind::Other,
                };

                first_logical_line_token = Some((logical_line_kind, *range));

                (logical_line_kind, *range)
            };

            if !token_kind.is_trivia() {
                line_is_comment_only = false;
            }

            // A docstring line is composed only of the docstring (TokenKind::String) and trivia tokens.
            // (If a comment follows a docstring, we still count the line as a docstring)
            if token_kind != TokenKind::String && !token_kind.is_trivia() {
                is_docstring = false;
            }

            match token_kind {
                TokenKind::Lbrace | TokenKind::Lpar | TokenKind::Lsqb => {
                    parens = parens.saturating_add(1);
                }
                TokenKind::Rbrace | TokenKind::Rpar | TokenKind::Rsqb => {
                    parens = parens.saturating_sub(1);
                }
                TokenKind::Newline | TokenKind::NonLogicalNewline if parens == 0 => {
                    let indent_range = TextRange::new(self.line_start, first_token_range.start());

                    let indent_length =
                        expand_indent(self.locator.slice(indent_range), self.indent_width);

                    self.max_preceding_blank_lines =
                        self.max_preceding_blank_lines.max(blank_lines);

                    let logical_line = LogicalLineInfo {
                        kind: logical_line_kind,
                        first_token_range,
                        last_token,
                        logical_line_end: range.end(),
                        is_comment_only: line_is_comment_only,
                        is_docstring,
                        indent_length,
                        blank_lines,
                        preceding_blank_lines: self.max_preceding_blank_lines,
                    };

                    // Reset the blank lines after a non-comment only line.
                    if !line_is_comment_only {
                        self.max_preceding_blank_lines = BlankLines::Zero;
                    }

                    // Set the start for the next logical line.
                    self.line_start = range.end();

                    return Some(logical_line);
                }
                _ => {}
            }

            last_token = token_kind;
        }

        None
    }
}

#[derive(Clone, Copy, Debug, Default)]
enum BlankLines {
    /// No blank lines
    #[default]
    Zero,

    /// One or more blank lines
    Many { count: NonZeroU32, range: TextRange },
}

impl BlankLines {
    fn add(&mut self, line_range: TextRange) {
        match self {
            BlankLines::Zero => {
                *self = BlankLines::Many {
                    count: NonZeroU32::MIN,
                    range: line_range,
                }
            }
            BlankLines::Many { count, range } => {
                *count = count.saturating_add(1);
                *range = TextRange::new(range.start(), line_range.end());
            }
        }
    }

    fn count(&self) -> u32 {
        match self {
            BlankLines::Zero => 0,
            BlankLines::Many { count, .. } => count.get(),
        }
    }

    fn range(&self) -> Option<TextRange> {
        match self {
            BlankLines::Zero => None,
            BlankLines::Many { range, .. } => Some(*range),
        }
    }
}

impl PartialEq<u32> for BlankLines {
    fn eq(&self, other: &u32) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}

impl PartialOrd<u32> for BlankLines {
    fn partial_cmp(&self, other: &u32) -> Option<Ordering> {
        self.count().partial_cmp(other)
    }
}

impl PartialOrd for BlankLines {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BlankLines {
    fn cmp(&self, other: &Self) -> Ordering {
        self.count().cmp(&other.count())
    }
}

impl PartialEq for BlankLines {
    fn eq(&self, other: &Self) -> bool {
        self.count() == other.count()
    }
}

impl Eq for BlankLines {}

#[derive(Copy, Clone, Debug, Default)]
enum Follows {
    #[default]
    Other,
    Decorator,
    Def,
    Docstring,
}

#[derive(Copy, Clone, Debug, Default)]
enum Status {
    /// Stores the indent level where the nesting started.
    Inside(usize),
    /// This is used to rectify a Inside switched to a Outside because of a dedented comment.
    CommentAfter(usize),
    #[default]
    Outside,
}

impl Status {
    fn update(&mut self, line: &LogicalLineInfo) {
        match *self {
            Status::Inside(nesting_indent) => {
                if line.indent_length <= nesting_indent {
                    if line.is_comment_only {
                        *self = Status::CommentAfter(nesting_indent);
                    } else {
                        *self = Status::Outside;
                    }
                }
            }
            Status::CommentAfter(indent) => {
                if !line.is_comment_only {
                    if line.indent_length > indent {
                        *self = Status::Inside(indent);
                    } else {
                        *self = Status::Outside;
                    }
                }
            }
            Status::Outside => {
                // Nothing to do
            }
        }
    }
}

/// Contains variables used for the linting of blank lines.
#[derive(Debug, Default)]
pub(crate) struct BlankLinesChecker {
    follows: Follows,
    fn_status: Status,
    class_status: Status,
    /// First line that is not a comment.
    is_not_first_logical_line: bool,
    /// Used for the fix in case a comment separates two non-comment logical lines to make the comment "stick"
    /// to the second line instead of the first.
    last_non_comment_line_end: TextSize,
    previous_unindented_line_kind: Option<LogicalLineKind>,
}

impl BlankLinesChecker {
    /// E301, E302, E303, E304, E305, E306
    pub(crate) fn check_lines(
        &mut self,
        tokens: &[LexResult],
        locator: &Locator,
        stylist: &Stylist,
        indent_width: IndentWidth,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut prev_indent_length: Option<usize> = None;
        let line_preprocessor = LinePreprocessor::new(tokens, locator, indent_width);

        for logical_line in line_preprocessor {
            self.check_line(
                &logical_line,
                prev_indent_length,
                locator,
                stylist,
                diagnostics,
            );
            if !logical_line.is_comment_only {
                prev_indent_length = Some(logical_line.indent_length);
            }
        }
    }

    #[allow(clippy::nonminimal_bool)]
    fn check_line(
        &mut self,
        line: &LogicalLineInfo,
        prev_indent_length: Option<usize>,
        locator: &Locator,
        stylist: &Stylist,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        self.class_status.update(line);
        self.fn_status.update(line);

        // Don't expect blank lines before the first non comment line.
        if self.is_not_first_logical_line {
            if line.preceding_blank_lines == 0
                // Only applies to methods.
                && matches!(line.kind,  LogicalLineKind::Function)
                && matches!(self.class_status, Status::Inside(_))
                // The class/parent method's docstring can directly precede the def.
                // Allow following a decorator (if there is an error it will be triggered on the first decorator).
                && !matches!(self.follows, Follows::Docstring | Follows::Decorator)
                // Do not trigger when the def follows an if/while/etc...
                && prev_indent_length.is_some_and(|prev_indent_length| prev_indent_length >= line.indent_length)
            {
                // E301
                let mut diagnostic =
                    Diagnostic::new(BlankLineBetweenMethods, line.first_token_range);
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist.line_ending().to_string(),
                    locator.line_start(self.last_non_comment_line_end),
                )));

                diagnostics.push(diagnostic);
            }

            if line.preceding_blank_lines < BLANK_LINES_TOP_LEVEL
                // Allow following a decorator (if there is an error it will be triggered on the first decorator).
                && !matches!(self.follows, Follows::Decorator)
                // Allow groups of one-liners.
                && !(matches!(self.follows, Follows::Def) && !matches!(line.last_token, TokenKind::Colon))
                // Only trigger on non-indented classes and functions (for example functions within an if are ignored)
                && line.indent_length == 0
                // Only apply to functions or classes.
                && line.kind.is_top_level()
            {
                // E302
                let mut diagnostic = Diagnostic::new(
                    BlankLinesTopLevel {
                        actual_blank_lines: line.preceding_blank_lines.count(),
                    },
                    line.first_token_range,
                );

                if let Some(blank_lines_range) = line.blank_lines.range() {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        stylist.line_ending().repeat(BLANK_LINES_TOP_LEVEL as usize),
                        blank_lines_range,
                    )));
                } else {
                    diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                        stylist.line_ending().repeat(BLANK_LINES_TOP_LEVEL as usize),
                        locator.line_start(self.last_non_comment_line_end),
                    )));
                }

                diagnostics.push(diagnostic);
            }

            let expected_blank_lines = if line.indent_length > 0 {
                BLANK_LINES_METHOD_LEVEL
            } else {
                BLANK_LINES_TOP_LEVEL
            };

            if line.blank_lines > expected_blank_lines {
                // E303
                let mut diagnostic = Diagnostic::new(
                    TooManyBlankLines {
                        actual_blank_lines: line.blank_lines.count(),
                    },
                    line.first_token_range,
                );

                if let Some(blank_lines_range) = line.blank_lines.range() {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        stylist.line_ending().repeat(expected_blank_lines as usize),
                        blank_lines_range,
                    )));
                }

                diagnostics.push(diagnostic);
            }

            if matches!(self.follows, Follows::Decorator)
                && !line.is_comment_only
                && line.preceding_blank_lines > 0
            {
                // E304
                let mut diagnostic = Diagnostic::new(
                    BlankLineAfterDecorator {
                        actual_blank_lines: line.preceding_blank_lines.count(),
                    },
                    line.first_token_range,
                );

                // Get all the lines between the last decorator line (included) and the current line (included).
                // Then remove all blank lines.
                let trivia_range = TextRange::new(
                    self.last_non_comment_line_end,
                    locator.line_start(line.first_token_range.start()),
                );
                let trivia_text = locator.slice(trivia_range);
                let mut trivia_without_blank_lines = trivia_text
                    .universal_newlines()
                    .filter_map(|line| {
                        (!line.trim_whitespace().is_empty()).then_some(line.as_str())
                    })
                    .join(&stylist.line_ending());

                let fix = if trivia_without_blank_lines.is_empty() {
                    Fix::safe_edit(Edit::range_deletion(trivia_range))
                } else {
                    trivia_without_blank_lines.push_str(&stylist.line_ending());
                    Fix::safe_edit(Edit::range_replacement(
                        trivia_without_blank_lines,
                        trivia_range,
                    ))
                };

                diagnostic.set_fix(fix);

                diagnostics.push(diagnostic);
            }

            if line.preceding_blank_lines < BLANK_LINES_TOP_LEVEL
                && self
                    .previous_unindented_line_kind
                    .is_some_and(LogicalLineKind::is_top_level)
                && line.indent_length == 0
                && !line.is_comment_only
                && !line.kind.is_top_level()
            {
                // E305
                let mut diagnostic = Diagnostic::new(
                    BlankLinesAfterFunctionOrClass {
                        actual_blank_lines: line.preceding_blank_lines.count(),
                    },
                    line.first_token_range,
                );

                if let Some(blank_lines_range) = line.blank_lines.range() {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        stylist.line_ending().repeat(BLANK_LINES_TOP_LEVEL as usize),
                        blank_lines_range,
                    )));
                } else {
                    diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                        stylist.line_ending().repeat(BLANK_LINES_TOP_LEVEL as usize),
                        locator.line_start(line.first_token_range.start()),
                    )));
                }

                diagnostics.push(diagnostic);
            }

            if line.preceding_blank_lines == 0
                // Only apply to nested functions.
                && matches!(self.fn_status, Status::Inside(_))
                && line.kind.is_top_level()
                // Allow following a decorator (if there is an error it will be triggered on the first decorator).
                && !matches!(self.follows, Follows::Decorator)
                // The class's docstring can directly precede the first function.
                && !matches!(self.follows, Follows::Docstring)
                // Do not trigger when the def/class follows an "indenting token" (if/while/etc...).
                && prev_indent_length.is_some_and(|prev_indent_length| prev_indent_length >= line.indent_length)
                // Allow groups of one-liners.
                && !(matches!(self.follows, Follows::Def) && line.last_token != TokenKind::Colon)
            {
                // E306
                let mut diagnostic =
                    Diagnostic::new(BlankLinesBeforeNestedDefinition, line.first_token_range);

                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist.line_ending().to_string(),
                    locator.line_start(line.first_token_range.start()),
                )));

                diagnostics.push(diagnostic);
            }
        }

        match line.kind {
            LogicalLineKind::Class => {
                if matches!(self.class_status, Status::Outside) {
                    self.class_status = Status::Inside(line.indent_length);
                }
                self.follows = Follows::Other;
            }
            LogicalLineKind::Decorator => {
                self.follows = Follows::Decorator;
            }
            LogicalLineKind::Function => {
                if matches!(self.fn_status, Status::Outside) {
                    self.fn_status = Status::Inside(line.indent_length);
                }
                self.follows = Follows::Def;
            }
            LogicalLineKind::Comment => {}
            LogicalLineKind::Other => {
                self.follows = Follows::Other;
            }
        }

        if line.is_docstring {
            self.follows = Follows::Docstring;
        }

        if !line.is_comment_only {
            self.is_not_first_logical_line = true;

            self.last_non_comment_line_end = line.logical_line_end;

            if line.indent_length == 0 {
                self.previous_unindented_line_kind = Some(line.kind);
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum LogicalLineKind {
    /// The clause header of a class definition
    Class,
    /// A decorator
    Decorator,
    /// The clause header of a function
    Function,
    /// A comment only line
    Comment,
    /// Any other statement or clause header
    Other,
}

impl LogicalLineKind {
    fn is_top_level(self) -> bool {
        matches!(
            self,
            LogicalLineKind::Class | LogicalLineKind::Function | LogicalLineKind::Decorator
        )
    }
}
