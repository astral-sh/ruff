use std::cmp::Ordering;
use std::iter::Peekable;
use std::num::NonZeroU32;
use std::slice::Iter;

use itertools::Itertools;

use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_notebook::CellOffsets;
use ruff_python_ast::PySourceType;
use ruff_python_codegen::Stylist;
use ruff_python_parser::TokenIterWithContext;
use ruff_python_parser::TokenKind;
use ruff_python_parser::Tokens;
use ruff_python_trivia::PythonWhitespace;
use ruff_source_file::{LineRanges, UniversalNewlines};
use ruff_text_size::TextRange;
use ruff_text_size::TextSize;

use crate::checkers::logical_lines::expand_indent;
use crate::line_width::IndentWidth;
use crate::rules::pycodestyle::helpers::is_non_logical_token;
use crate::Locator;

/// Number of blank lines around top level classes and functions.
const BLANK_LINES_TOP_LEVEL: u32 = 2;
/// Number of blank lines around methods and nested classes and functions.
const BLANK_LINES_NESTED_LEVEL: u32 = 1;

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
/// ## Typing stub files (`.pyi`)
/// The typing style guide recommends to not use blank lines between methods except to group
/// them. That's why this rule is not enabled in typing stub files.
///
/// ## References
/// - [PEP 8: Blank Lines](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E301.html)
/// - [Typing Style Guide](https://typing.readthedocs.io/en/latest/source/stubs.html#blank-lines)
#[derive(ViolationMetadata)]
pub(crate) struct BlankLineBetweenMethods;

impl AlwaysFixableViolation for BlankLineBetweenMethods {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected {BLANK_LINES_NESTED_LEVEL:?} blank line, found 0")
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
/// The rule respects the [`lint.isort.lines-after-imports`] setting when
/// determining the required number of blank lines between top-level `import`
/// statements and function or class definitions for compatibility with isort.
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
/// ## Typing stub files (`.pyi`)
/// The typing style guide recommends to not use blank lines between classes and functions except to group
/// them. That's why this rule is not enabled in typing stub files.
///
/// ## Options
/// - `lint.isort.lines-after-imports`
///
/// ## References
/// - [PEP 8: Blank Lines](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E302.html)
/// - [Typing Style Guide](https://typing.readthedocs.io/en/latest/source/stubs.html#blank-lines)
#[derive(ViolationMetadata)]
pub(crate) struct BlankLinesTopLevel {
    actual_blank_lines: u32,
    expected_blank_lines: u32,
}

impl AlwaysFixableViolation for BlankLinesTopLevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesTopLevel {
            actual_blank_lines,
            expected_blank_lines,
        } = self;

        format!("Expected {expected_blank_lines:?} blank lines, found {actual_blank_lines}")
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
/// ## Typing stub files (`.pyi`)
/// The rule allows at most one blank line in typing stub files in accordance to the typing style guide recommendation.
///
/// Note: The rule respects the following `isort` settings when determining the maximum number of blank lines allowed between two statements:
///
/// * [`lint.isort.lines-after-imports`]: For top-level statements directly following an import statement.
/// * [`lint.isort.lines-between-types`]: For `import` statements directly following a `from ... import ...` statement or vice versa.
///
/// ## Options
/// - `lint.isort.lines-after-imports`
/// - `lint.isort.lines-between-types`
///
/// ## References
/// - [PEP 8: Blank Lines](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E303.html)
/// - [Typing Style Guide](https://typing.readthedocs.io/en/latest/source/stubs.html#blank-lines)
#[derive(ViolationMetadata)]
pub(crate) struct TooManyBlankLines {
    actual_blank_lines: u32,
}

impl AlwaysFixableViolation for TooManyBlankLines {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBlankLines { actual_blank_lines } = self;

        format!("Too many blank lines ({actual_blank_lines})")
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
/// - [PEP 8: Blank Lines](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E304.html)
#[derive(ViolationMetadata)]
pub(crate) struct BlankLineAfterDecorator {
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
/// PEP 8 recommends using blank lines as follows:
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
/// ## Typing stub files (`.pyi`)
/// The typing style guide recommends to not use blank lines between statements except to group
/// them. That's why this rule is not enabled in typing stub files.
///
/// ## References
/// - [PEP 8: Blank Lines](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E305.html)
/// - [Typing Style Guide](https://typing.readthedocs.io/en/latest/source/stubs.html#blank-lines)
#[derive(ViolationMetadata)]
pub(crate) struct BlankLinesAfterFunctionOrClass {
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
/// PEP 8 recommends using blank lines as follows:
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
/// ## Typing stub files (`.pyi`)
/// The typing style guide recommends to not use blank lines between classes and functions except to group
/// them. That's why this rule is not enabled in typing stub files.
///
/// ## References
/// - [PEP 8: Blank Lines](https://peps.python.org/pep-0008/#blank-lines)
/// - [Flake 8 rule](https://www.flake8rules.com/rules/E306.html)
/// - [Typing Style Guide](https://typing.readthedocs.io/en/latest/source/stubs.html#blank-lines)
#[derive(ViolationMetadata)]
pub(crate) struct BlankLinesBeforeNestedDefinition;

impl AlwaysFixableViolation for BlankLinesBeforeNestedDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Expected 1 blank line before a nested definition, found 0".to_string()
    }

    fn fix_title(&self) -> String {
        "Add missing blank line".to_string()
    }
}

#[derive(Debug)]
struct LogicalLineInfo {
    kind: LogicalLineKind,
    first_token_range: TextRange,

    /// The kind of the last non-trivia token before the newline ending the logical line.
    last_token: TokenKind,

    /// The end of the logical line including the newline.
    logical_line_end: TextSize,

    /// `true` if this is not a blank but only consists of a comment.
    is_comment_only: bool,

    /// If running on a notebook, whether the line is the first logical line (or a comment preceding it) of its cell.
    is_beginning_of_cell: bool,

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
    tokens: TokenIterWithContext<'a>,
    locator: &'a Locator<'a>,
    indent_width: IndentWidth,
    /// The start position of the next logical line.
    line_start: TextSize,
    /// Maximum number of consecutive blank lines between the current line and the previous non-comment logical line.
    /// One of its main uses is to allow a comment to directly precede a class/function definition.
    max_preceding_blank_lines: BlankLines,
    /// The cell offsets of the notebook (if running on a notebook).
    cell_offsets: Option<Peekable<Iter<'a, TextSize>>>,
    /// If running on a notebook, whether the line is the first logical line (or a comment preceding it) of its cell.
    is_beginning_of_cell: bool,
}

impl<'a> LinePreprocessor<'a> {
    fn new(
        tokens: &'a Tokens,
        locator: &'a Locator,
        indent_width: IndentWidth,
        cell_offsets: Option<&'a CellOffsets>,
    ) -> LinePreprocessor<'a> {
        LinePreprocessor {
            tokens: tokens.iter_with_context(),
            locator,
            line_start: TextSize::new(0),
            max_preceding_blank_lines: BlankLines::Zero,
            indent_width,
            is_beginning_of_cell: cell_offsets.is_some(),
            cell_offsets: cell_offsets
                .map(|cell_offsets| cell_offsets.get(1..).unwrap_or_default().iter().peekable()),
        }
    }
}

impl Iterator for LinePreprocessor<'_> {
    type Item = LogicalLineInfo;

    fn next(&mut self) -> Option<LogicalLineInfo> {
        let mut line_is_comment_only = true;
        let mut is_docstring = false;
        // Number of consecutive blank lines directly preceding this logical line.
        let mut blank_lines = BlankLines::Zero;
        let mut first_logical_line_token: Option<(LogicalLineKind, TextRange)> = None;
        let mut last_token = TokenKind::EndOfFile;

        while let Some(token) = self.tokens.next() {
            let (kind, range) = token.as_tuple();
            if matches!(kind, TokenKind::Indent | TokenKind::Dedent) {
                continue;
            }

            let (logical_line_kind, first_token_range) =
                if let Some(first_token_range) = first_logical_line_token {
                    first_token_range
                }
                // At the start of the line...
                else {
                    // Check if we are at the beginning of a cell in a notebook.
                    if let Some(ref mut cell_offsets) = self.cell_offsets {
                        if cell_offsets
                            .peek()
                            .is_some_and(|offset| offset == &&self.line_start)
                        {
                            self.is_beginning_of_cell = true;
                            cell_offsets.next();
                            blank_lines = BlankLines::Zero;
                            self.max_preceding_blank_lines = BlankLines::Zero;
                        }
                    }

                    // An empty line
                    if kind == TokenKind::NonLogicalNewline {
                        blank_lines.add(range);

                        self.line_start = range.end();

                        continue;
                    }

                    is_docstring = kind == TokenKind::String;

                    let logical_line_kind = match kind {
                        TokenKind::Class => LogicalLineKind::Class,
                        TokenKind::Comment => LogicalLineKind::Comment,
                        TokenKind::At => LogicalLineKind::Decorator,
                        TokenKind::Def => LogicalLineKind::Function,
                        // Lookahead to distinguish `async def` from `async with`.
                        TokenKind::Async
                            if self
                                .tokens
                                .peek()
                                .is_some_and(|token| token.kind() == TokenKind::Def) =>
                        {
                            LogicalLineKind::Function
                        }
                        TokenKind::Import => LogicalLineKind::Import,
                        TokenKind::From => LogicalLineKind::FromImport,
                        _ => LogicalLineKind::Other,
                    };

                    first_logical_line_token = Some((logical_line_kind, range));

                    (logical_line_kind, range)
                };

            if !is_non_logical_token(kind) {
                line_is_comment_only = false;
            }

            // A docstring line is composed only of the docstring (TokenKind::String) and trivia tokens.
            // (If a comment follows a docstring, we still count the line as a docstring)
            if kind != TokenKind::String && !is_non_logical_token(kind) {
                is_docstring = false;
            }

            if kind.is_any_newline() && !self.tokens.in_parenthesized_context() {
                let indent_range = TextRange::new(self.line_start, first_token_range.start());

                let indent_length =
                    expand_indent(self.locator.slice(indent_range), self.indent_width);

                self.max_preceding_blank_lines = self.max_preceding_blank_lines.max(blank_lines);

                let logical_line = LogicalLineInfo {
                    kind: logical_line_kind,
                    first_token_range,
                    last_token,
                    logical_line_end: range.end(),
                    is_comment_only: line_is_comment_only,
                    is_beginning_of_cell: self.is_beginning_of_cell,
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

                if self.cell_offsets.is_some() && !line_is_comment_only {
                    self.is_beginning_of_cell = false;
                }

                return Some(logical_line);
            }

            if !is_non_logical_token(kind) {
                last_token = kind;
            }
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
    /// A function whose body is a dummy (...), if the ellipsis is on the same line as the def.
    DummyDef,
    Import,
    FromImport,
    Docstring,
}

impl Follows {
    // Allow a function/method to follow a function/method with a dummy body.
    const fn follows_def_with_dummy_body(self) -> bool {
        matches!(self, Follows::DummyDef)
    }
}

impl Follows {
    const fn is_any_def(self) -> bool {
        matches!(self, Follows::Def | Follows::DummyDef)
    }
}

impl Follows {
    const fn is_any_import(self) -> bool {
        matches!(self, Follows::Import | Follows::FromImport)
    }
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
#[derive(Debug)]
pub(crate) struct BlankLinesChecker<'a> {
    stylist: &'a Stylist<'a>,
    locator: &'a Locator<'a>,
    indent_width: IndentWidth,
    lines_after_imports: isize,
    lines_between_types: usize,
    source_type: PySourceType,
    cell_offsets: Option<&'a CellOffsets>,
}

impl<'a> BlankLinesChecker<'a> {
    pub(crate) fn new(
        locator: &'a Locator<'a>,
        stylist: &'a Stylist<'a>,
        settings: &crate::settings::LinterSettings,
        source_type: PySourceType,
        cell_offsets: Option<&'a CellOffsets>,
    ) -> BlankLinesChecker<'a> {
        BlankLinesChecker {
            stylist,
            locator,
            indent_width: settings.tab_size,
            lines_after_imports: settings.isort.lines_after_imports,
            lines_between_types: settings.isort.lines_between_types,
            source_type,
            cell_offsets,
        }
    }

    /// E301, E302, E303, E304, E305, E306
    pub(crate) fn check_lines(&self, tokens: &Tokens, diagnostics: &mut Vec<Diagnostic>) {
        let mut prev_indent_length: Option<usize> = None;
        let mut prev_logical_line: Option<LogicalLineInfo> = None;
        let mut state = BlankLinesState::default();
        let line_preprocessor =
            LinePreprocessor::new(tokens, self.locator, self.indent_width, self.cell_offsets);

        for logical_line in line_preprocessor {
            // Reset `follows` after a dedent:
            // ```python
            // if True:
            //      import test
            // a = 10
            // ```
            // The `a` statement doesn't follow the `import` statement but the `if` statement.
            if let Some(prev_indent_length) = prev_indent_length {
                if prev_indent_length > logical_line.indent_length {
                    state.follows = Follows::Other;
                }
            }

            // Reset the previous line end after an indent or dedent:
            // ```python
            // if True:
            //      import test
            //      # comment
            // a = 10
            // ```
            // The `# comment` should be attached to the `import` statement, rather than the
            // assignment.
            if let Some(prev_logical_line) = prev_logical_line {
                if prev_logical_line.is_comment_only {
                    if prev_logical_line.indent_length != logical_line.indent_length {
                        state.last_non_comment_line_end = prev_logical_line.logical_line_end;
                    }
                }
            }

            state.class_status.update(&logical_line);
            state.fn_status.update(&logical_line);

            self.check_line(&logical_line, &state, prev_indent_length, diagnostics);

            match logical_line.kind {
                LogicalLineKind::Class => {
                    if matches!(state.class_status, Status::Outside) {
                        state.class_status = Status::Inside(logical_line.indent_length);
                    }
                    state.follows = Follows::Other;
                }
                LogicalLineKind::Decorator => {
                    state.follows = Follows::Decorator;
                }
                LogicalLineKind::Function => {
                    if matches!(state.fn_status, Status::Outside) {
                        state.fn_status = Status::Inside(logical_line.indent_length);
                    }
                    state.follows = if logical_line.last_token == TokenKind::Ellipsis {
                        Follows::DummyDef
                    } else {
                        Follows::Def
                    };
                }
                LogicalLineKind::Comment => {}
                LogicalLineKind::Import => {
                    state.follows = Follows::Import;
                }
                LogicalLineKind::FromImport => {
                    state.follows = Follows::FromImport;
                }
                LogicalLineKind::Other => {
                    state.follows = Follows::Other;
                }
            }

            if logical_line.is_docstring {
                state.follows = Follows::Docstring;
            }

            if !logical_line.is_comment_only {
                state.is_not_first_logical_line = true;

                state.last_non_comment_line_end = logical_line.logical_line_end;

                if logical_line.indent_length == 0 {
                    state.previous_unindented_line_kind = Some(logical_line.kind);
                }
            }

            if !logical_line.is_comment_only {
                prev_indent_length = Some(logical_line.indent_length);
            }

            prev_logical_line = Some(logical_line);
        }
    }

    #[allow(clippy::nonminimal_bool)]
    fn check_line(
        &self,
        line: &LogicalLineInfo,
        state: &BlankLinesState,
        prev_indent_length: Option<usize>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if line.preceding_blank_lines == 0
            // Only applies to methods.
            && matches!(line.kind,  LogicalLineKind::Function | LogicalLineKind::Decorator)
            // Allow groups of one-liners.
            && !(state.follows.is_any_def() && line.last_token != TokenKind::Colon)
            && !state.follows.follows_def_with_dummy_body()
            && matches!(state.class_status, Status::Inside(_))
            // The class/parent method's docstring can directly precede the def.
            // Allow following a decorator (if there is an error it will be triggered on the first decorator).
            && !matches!(state.follows, Follows::Docstring | Follows::Decorator)
            // Do not trigger when the def follows an if/while/etc...
            && prev_indent_length.is_some_and(|prev_indent_length| prev_indent_length >= line.indent_length)
            // Blank lines in stub files are only used for grouping. Don't enforce blank lines.
            && !self.source_type.is_stub()
        {
            // E301
            let mut diagnostic = Diagnostic::new(BlankLineBetweenMethods, line.first_token_range);
            diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                self.stylist.line_ending().to_string(),
                self.locator.line_start(state.last_non_comment_line_end),
            )));

            diagnostics.push(diagnostic);
        }

        // Blank lines in stub files are used to group definitions. Don't enforce blank lines.
        let max_lines_level = if self.source_type.is_stub() {
            1
        } else {
            if line.indent_length == 0 {
                BLANK_LINES_TOP_LEVEL
            } else {
                BLANK_LINES_NESTED_LEVEL
            }
        };

        let expected_blank_lines_before_definition = if line.indent_length == 0 {
            // Mimic the isort rules for the number of blank lines before classes and functions
            if state.follows.is_any_import() {
                // Fallback to the default if the value is too large for an u32 or if it is negative.
                // A negative value means that isort should determine the blank lines automatically.
                // `isort` defaults to 2 if before a class or function definition (except in stubs where it is one) and 1 otherwise.
                // Defaulting to 2 (or 1 in stubs) here is correct because the variable is only used when testing the
                // blank lines before a class or function definition.
                u32::try_from(self.lines_after_imports).unwrap_or(max_lines_level)
            } else {
                max_lines_level
            }
        } else {
            max_lines_level
        };

        if line.preceding_blank_lines < expected_blank_lines_before_definition
            // Allow following a decorator (if there is an error it will be triggered on the first decorator).
            && !matches!(state.follows, Follows::Decorator)
            // Allow groups of one-liners.
            && !(state.follows.is_any_def() && line.last_token != TokenKind::Colon)
            && !(state.follows.follows_def_with_dummy_body() && line.preceding_blank_lines == 0)
            // Only trigger on non-indented classes and functions (for example functions within an if are ignored)
            && line.indent_length == 0
            // Only apply to functions or classes.
            && line.kind.is_class_function_or_decorator()
            // Blank lines in stub files are used to group definitions. Don't enforce blank lines.
            && !self.source_type.is_stub()
            // Do not expect blank lines before the first logical line.
            && state.is_not_first_logical_line
            // Ignore the first logical line (and any comment preceding it) of each cell in notebooks.
            && !line.is_beginning_of_cell
        {
            // E302
            let mut diagnostic = Diagnostic::new(
                BlankLinesTopLevel {
                    actual_blank_lines: line.preceding_blank_lines.count(),
                    expected_blank_lines: expected_blank_lines_before_definition,
                },
                line.first_token_range,
            );

            if let Some(blank_lines_range) = line.blank_lines.range() {
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    self.stylist
                        .line_ending()
                        .repeat(expected_blank_lines_before_definition as usize),
                    blank_lines_range,
                )));
            } else {
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    self.stylist.line_ending().repeat(
                        (expected_blank_lines_before_definition
                            - line.preceding_blank_lines.count()) as usize,
                    ),
                    self.locator.line_start(state.last_non_comment_line_end),
                )));
            }

            diagnostics.push(diagnostic);
        }

        // If between `import` and `from .. import ..` or the other way round,
        // allow up to `lines_between_types` newlines for isort compatibility.
        // We let `isort` remove extra blank lines when the imports belong
        // to different sections.
        let max_blank_lines = if matches!(
            (line.kind, state.follows),
            (LogicalLineKind::Import, Follows::FromImport)
                | (LogicalLineKind::FromImport, Follows::Import)
        ) {
            max_lines_level.max(u32::try_from(self.lines_between_types).unwrap_or(u32::MAX))
        } else {
            expected_blank_lines_before_definition
        };

        if line.blank_lines > max_blank_lines {
            // E303
            let mut diagnostic = Diagnostic::new(
                TooManyBlankLines {
                    actual_blank_lines: line.blank_lines.count(),
                },
                line.first_token_range,
            );

            if let Some(blank_lines_range) = line.blank_lines.range() {
                if max_blank_lines == 0 {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(blank_lines_range)));
                } else {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        self.stylist.line_ending().repeat(max_blank_lines as usize),
                        blank_lines_range,
                    )));
                }
            }

            diagnostics.push(diagnostic);
        }

        if matches!(state.follows, Follows::Decorator)
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
                state.last_non_comment_line_end,
                self.locator.line_start(line.first_token_range.start()),
            );
            let trivia_text = self.locator.slice(trivia_range);
            let mut trivia_without_blank_lines = trivia_text
                .universal_newlines()
                .filter_map(|line| (!line.trim_whitespace().is_empty()).then_some(line.as_str()))
                .join(&self.stylist.line_ending());

            let fix = if trivia_without_blank_lines.is_empty() {
                Fix::safe_edit(Edit::range_deletion(trivia_range))
            } else {
                trivia_without_blank_lines.push_str(&self.stylist.line_ending());
                Fix::safe_edit(Edit::range_replacement(
                    trivia_without_blank_lines,
                    trivia_range,
                ))
            };

            diagnostic.set_fix(fix);

            diagnostics.push(diagnostic);
        }

        if line.preceding_blank_lines < BLANK_LINES_TOP_LEVEL
            && state
                .previous_unindented_line_kind
                .is_some_and(LogicalLineKind::is_class_function_or_decorator)
            && line.indent_length == 0
            && !line.is_comment_only
            && !line.kind.is_class_function_or_decorator()
            // Blank lines in stub files are used for grouping, don't enforce blank lines.
            && !self.source_type.is_stub()
            // Ignore the first logical line (and any comment preceding it) of each cell in notebooks.
            && !line.is_beginning_of_cell
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
                    self.stylist
                        .line_ending()
                        .repeat(BLANK_LINES_TOP_LEVEL as usize),
                    blank_lines_range,
                )));
            } else {
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    self.stylist.line_ending().repeat(
                        (BLANK_LINES_TOP_LEVEL - line.preceding_blank_lines.count()) as usize,
                    ),
                    self.locator.line_start(state.last_non_comment_line_end),
                )));
            }

            diagnostics.push(diagnostic);
        }

        if line.preceding_blank_lines == 0
            // Only apply to nested functions.
            && matches!(state.fn_status, Status::Inside(_))
            && line.kind.is_class_function_or_decorator()
            // Allow following a decorator (if there is an error it will be triggered on the first decorator).
            && !matches!(state.follows, Follows::Decorator)
            // The class's docstring can directly precede the first function.
            && !matches!(state.follows, Follows::Docstring)
            // Do not trigger when the def/class follows an "indenting token" (if/while/etc...).
            && prev_indent_length.is_some_and(|prev_indent_length| prev_indent_length >= line.indent_length)
            // Allow groups of one-liners.
            && !(state.follows.is_any_def() && line.last_token != TokenKind::Colon)
            && !state.follows.follows_def_with_dummy_body()
            // Blank lines in stub files are only used for grouping. Don't enforce blank lines.
            && !self.source_type.is_stub()
        {
            // E306
            let mut diagnostic =
                Diagnostic::new(BlankLinesBeforeNestedDefinition, line.first_token_range);

            diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                self.stylist.line_ending().to_string(),
                self.locator.line_start(line.first_token_range.start()),
            )));

            diagnostics.push(diagnostic);
        }
    }
}

#[derive(Clone, Debug, Default)]
struct BlankLinesState {
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
    /// An import statement
    Import,
    /// A from.. import statement
    FromImport,
    /// Any other statement or clause header
    Other,
}

impl LogicalLineKind {
    fn is_class_function_or_decorator(self) -> bool {
        matches!(
            self,
            LogicalLineKind::Class | LogicalLineKind::Function | LogicalLineKind::Decorator
        )
    }
}
