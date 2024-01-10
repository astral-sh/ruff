use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_python_parser::Tok;
use std::iter::Flatten;
use std::slice::Iter;

use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Stylist;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::lexer::LexicalError;
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;

use ruff_text_size::TextRange;
use ruff_text_size::TextSize;

use crate::checkers::logical_lines::expand_indent;

/// Number of blank lines around top level classes and functions.
const BLANK_LINES_TOP_LEVEL: u32 = 2;
/// Number of blank lines around methods and nested classes and functions.
const BLANK_LINES_METHOD_LEVEL: u32 = 1;

/// ## What it does
/// Checks for missing blank lines between methods of a class.
///
/// ## Why is this bad?
/// PEP 8 recommends the use of blank lines as follows:
/// - Two blank lines are expected between functions and classes
/// - One blank line is expected between methods of a class.
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
pub struct BlankLineBetweenMethods {
    actual_blank_lines: u32,
}

impl AlwaysFixableViolation for BlankLineBetweenMethods {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineBetweenMethods {
            actual_blank_lines: nb_blank_lines,
        } = self;
        format!("Expected {BLANK_LINES_METHOD_LEVEL:?} blank line, found {nb_blank_lines}")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for missing blank lines between top level functions and classes.
///
/// ## Why is this bad?
/// PEP 8 recommends the use of blank lines as follows:
/// - Two blank lines are expected between functions and classes
/// - One blank line is expected between methods of a class.
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
/// Checks for missing blank line after function decorator.
///
/// ## Why is this bad?
/// PEP 8 recommends the use of blank lines as follows:
/// - Two blank lines are expected between functions and classes
/// - One blank line is expected between methods of a class.
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
pub struct BlankLineAfterDecorator;

impl AlwaysFixableViolation for BlankLineAfterDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("blank lines found after function decorator")
    }

    fn fix_title(&self) -> String {
        "Remove extraneous blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for missing blank lines after end of function or class.
///
/// ## Why is this bad?
/// PEP 8 recommends the using blank lines as following:
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
        format!("expected 2 blank lines after class or function definition, found ({blank_lines})")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for for 1 blank line between nested functions/classes definitions.
///
/// ## Why is this bad?
/// PEP 8 recommends the using blank lines as following:
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
pub struct BlankLinesBeforeNestedDefinition {
    actual_blank_lines: u32,
}

impl AlwaysFixableViolation for BlankLinesBeforeNestedDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesBeforeNestedDefinition {
            actual_blank_lines: blank_lines,
        } = self;
        format!("Expected 1 blank line before a nested definition, found {blank_lines}")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line".to_string()
    }
}

/// Returns `true` if the token is a top level token.
/// It is sufficient to test for Class and Def since the `LinePreprocessor` ignores Async tokens.
fn is_top_level_token(token: Option<TokenKind>) -> bool {
    matches!(&token, Some(TokenKind::Class | TokenKind::Def))
}

/// Returns `true` if the token is At, Async, Class or Def
fn is_top_level_token_or_decorator(token: TokenKind) -> bool {
    matches!(&token, TokenKind::Class | TokenKind::Def | TokenKind::At)
}

#[derive(Debug)]
struct LogicalLineInfo {
    first_token: TokenKind,
    first_token_range: TextRange,
    last_token: TokenKind,
    last_token_range: TextRange,
    is_comment_only: bool,
    is_docstring: bool,
    indent_level: usize,
    blank_lines: u32,
    preceding_blank_lines: u32,
    preceding_blank_characters: usize,
}

/// Iterator that processes tokens until a full logical line (or comment line) is "built".
/// It then returns characteristics of that logical line (see `LogicalLineInfo`).
struct LinePreprocessor<'a> {
    tokens: Flatten<Iter<'a, Result<(Tok, TextRange), LexicalError>>>,
    locator: &'a Locator<'a>,
    /// Number of previous consecutive blank lines.
    previous_blank_lines: u32,
    /// Number of consecutive blank lines.
    current_blank_lines: u32,
    /// Number of blank characters in the blank lines (\n vs \r\n for example).
    current_blank_characters: usize,
}

impl<'a> LinePreprocessor<'a> {
    fn new(tokens: &'a [LexResult], locator: &'a Locator) -> LinePreprocessor<'a> {
        LinePreprocessor {
            tokens: tokens.iter().flatten(),
            locator,
            previous_blank_lines: 0,
            current_blank_lines: 0,
            current_blank_characters: 0,
        }
    }
}

impl<'a> Iterator for LinePreprocessor<'a> {
    type Item = LogicalLineInfo;

    fn next(&mut self) -> Option<LogicalLineInfo> {
        let mut line_is_comment_only = true;
        let mut is_docstring = true;
        let mut first_token: Option<TokenKind> = None;
        let mut first_token_range: Option<TextRange> = None;
        let mut last_token: Option<TokenKind> = None;
        let mut parens = 0u32;

        while let Some((token, range)) = self.tokens.next() {
            let token = TokenKind::from_token(token);
            if matches!(token, TokenKind::Indent | TokenKind::Dedent) {
                continue;
            }

            if token != TokenKind::Newline {
                // Ideally, we would like to have a "async def" token since we care about the "def" part.
                // As a work around, we ignore the first token if it is "async".
                if first_token.is_none() && token != TokenKind::Async {
                    first_token = Some(token);
                }
                if first_token_range.is_none() {
                    first_token_range = Some(*range);
                }

                if !token.is_trivia() {
                    line_is_comment_only = false;
                }

                // A docstring line is composed only of the docstring (TokenKind::String) and trivia tokens.
                // (If a comment follows a docstring, we still count the line as a docstring)
                if token != TokenKind::String && !token.is_trivia() {
                    is_docstring = false;
                }

                last_token = Some(token);
            }

            match token {
                TokenKind::Lbrace | TokenKind::Lpar | TokenKind::Lsqb => {
                    parens = parens.saturating_add(1);
                }
                TokenKind::Rbrace | TokenKind::Rpar | TokenKind::Rsqb => {
                    parens = parens.saturating_sub(1);
                }
                TokenKind::Newline | TokenKind::NonLogicalNewline if parens == 0 => {
                    let last_token_range = *range;

                    // For a line to be a docstring, the first token must be a string (since indents are ignored)
                    if first_token != Some(TokenKind::String) {
                        is_docstring = false;
                    }

                    let first_range = first_token_range
                        .expect("that Newline cannot be the first token of a line (there is at least a NonLogicalNewline token)");

                    let range = TextRange::new(
                        self.locator.line_start(first_range.start()),
                        first_range.start(),
                    );

                    let indent_level = expand_indent(self.locator.slice(range));

                    // Empty line
                    if first_token == Some(TokenKind::NonLogicalNewline) {
                        self.current_blank_lines += 1;
                        self.current_blank_characters +=
                            range.end().to_usize() - first_range.start().to_usize() + 1;
                        return self.next();
                    }

                    if self.previous_blank_lines < self.current_blank_lines {
                        self.previous_blank_lines = self.current_blank_lines;
                    }

                    let logical_line = LogicalLineInfo {
                        first_token: first_token
                            .expect("that Newline cannot be the first token of a line (there is at least a NonLogicalNewline token)"),
                        first_token_range: first_range,
                        last_token: last_token
                            .expect("that Newline cannot be the first token of a line (there is at least a NonLogicalNewline token)"),
                        last_token_range,
                        is_comment_only: line_is_comment_only,
                        is_docstring,
                        indent_level,
                        blank_lines: self.current_blank_lines,
                        preceding_blank_lines: self.previous_blank_lines,
                        preceding_blank_characters: self.current_blank_characters,
                    };

                    if !line_is_comment_only {
                        self.previous_blank_lines = 0;
                    }
                    self.current_blank_lines = 0;
                    self.current_blank_characters = 0;
                    return Some(logical_line);
                }
                _ => {}
            }
        }

        None
    }
}

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
    previous_unindented_token: Option<TokenKind>,
}

impl BlankLinesChecker {
    /// E301, E302, E303, E304, E305, E306
    pub(crate) fn check_lines(
        &mut self,
        tokens: &[LexResult],
        locator: &Locator,
        stylist: &Stylist,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut prev_indent_level: Option<usize> = None;
        let line_preprocessor = LinePreprocessor::new(tokens, locator);

        for logical_line in line_preprocessor {
            self.check_line(
                &logical_line,
                prev_indent_level,
                locator,
                stylist,
                diagnostics,
            );
            if !logical_line.is_comment_only {
                prev_indent_level = Some(logical_line.indent_level);
            }
        }
    }

    #[allow(clippy::nonminimal_bool)]
    fn check_line(
        &mut self,
        line: &LogicalLineInfo,
        prev_indent_level: Option<usize>,
        locator: &Locator,
        stylist: &Stylist,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match self.class_status {
            Status::Inside(nesting_indent) => {
                if line.indent_level <= nesting_indent {
                    if line.is_comment_only {
                        self.class_status = Status::CommentAfter(nesting_indent);
                    } else {
                        self.class_status = Status::Outside;
                    }
                }
            }
            Status::CommentAfter(indent) => {
                if !line.is_comment_only {
                    if line.indent_level > indent {
                        self.class_status = Status::Inside(indent);
                    }
                    self.class_status = Status::Outside;
                }
            }
            Status::Outside => {
                // Nothing to do
            }
        }

        if let Status::Inside(nesting_indent) = self.fn_status {
            if line.indent_level <= nesting_indent {
                if line.is_comment_only {
                    self.fn_status = Status::CommentAfter(nesting_indent);
                } else {
                    self.fn_status = Status::Outside;
                }
            }
        }

        // A comment can be de-indented while still being in a class/function, in that case
        // we need to revert the variables.
        if !line.is_comment_only {
            if let Status::CommentAfter(indent) = self.fn_status {
                if line.indent_level > indent {
                    self.fn_status = Status::Inside(indent);
                } else {
                    self.fn_status = Status::Outside;
                }
            }
        }

        // Don't expect blank lines before the first non comment line.
        if self.is_not_first_logical_line {
            if line.preceding_blank_lines == 0
                // Only applies to methods.
                && line.first_token == TokenKind::Def
                && matches!(self.class_status, Status::Inside(_))
                // The class/parent method's docstring can directly precede the def.
                && !matches!(self.follows, Follows::Docstring)
                // Do not trigger when the def follows an if/while/etc...
                && prev_indent_level.is_some_and(|prev_indent_level| prev_indent_level >= line.indent_level)
                // Allow following a decorator (if there is an error it will be triggered on the first decorator).
                && !matches!(self.follows, Follows::Decorator)
            {
                // E301
                let mut diagnostic = Diagnostic::new(
                    BlankLineBetweenMethods {
                        actual_blank_lines: line.preceding_blank_lines,
                    },
                    line.first_token_range,
                );
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist.line_ending().as_str().to_string(),
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
                && line.indent_level == 0
                // Only apply to functions or classes.
                && is_top_level_token_or_decorator(line.first_token)
            {
                // E302
                let mut diagnostic = Diagnostic::new(
                    BlankLinesTopLevel {
                        actual_blank_lines: line.preceding_blank_lines,
                    },
                    line.first_token_range,
                );
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist
                        .line_ending()
                        .as_str()
                        .to_string()
                        .repeat((BLANK_LINES_TOP_LEVEL - line.preceding_blank_lines) as usize),
                    locator.line_start(self.last_non_comment_line_end),
                )));

                diagnostics.push(diagnostic);
            }

            if line.blank_lines > BLANK_LINES_TOP_LEVEL
                || (line.indent_level > 0 && line.blank_lines > BLANK_LINES_METHOD_LEVEL)
            {
                // E303
                let mut diagnostic = Diagnostic::new(
                    TooManyBlankLines {
                        actual_blank_lines: line.blank_lines,
                    },
                    line.first_token_range,
                );

                let chars_to_remove = if line.indent_level > 0 {
                    u32::try_from(line.preceding_blank_characters)
                        .expect("Number of blank characters to be small.")
                        - BLANK_LINES_METHOD_LEVEL
                } else {
                    u32::try_from(line.preceding_blank_characters)
                        .expect("Number of blank characters to be small.")
                        - BLANK_LINES_TOP_LEVEL
                };
                let end = locator.line_start(line.first_token_range.start());
                let start = end - TextSize::new(chars_to_remove);
                diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));

                diagnostics.push(diagnostic);
            }

            if matches!(self.follows, Follows::Decorator) && line.preceding_blank_lines > 0 {
                // E304
                let mut diagnostic =
                    Diagnostic::new(BlankLineAfterDecorator, line.first_token_range);

                let range = line.first_token_range;
                diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                    locator.line_start(range.start())
                        - TextSize::new(
                            line.preceding_blank_characters
                                .try_into()
                                .expect("Number of blank characters to be small."),
                        ),
                    locator.line_start(range.start()),
                )));

                diagnostics.push(diagnostic);
            }

            if line.preceding_blank_lines < BLANK_LINES_TOP_LEVEL
                && is_top_level_token(self.previous_unindented_token)
                && line.indent_level == 0
                && !line.is_comment_only
                && !is_top_level_token_or_decorator(line.first_token)
            {
                // E305
                let mut diagnostic = Diagnostic::new(
                    BlankLinesAfterFunctionOrClass {
                        actual_blank_lines: line.blank_lines,
                    },
                    line.first_token_range,
                );

                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist
                        .line_ending()
                        .as_str()
                        .to_string()
                        .repeat((BLANK_LINES_TOP_LEVEL - line.blank_lines) as usize),
                    locator.line_start(line.first_token_range.start()),
                )));

                diagnostics.push(diagnostic);
            }

            if line.preceding_blank_lines == 0
            // Only apply to nested functions.
                && matches!(self.fn_status, Status::Inside(_))
                && is_top_level_token_or_decorator(line.first_token)
                // Allow following a decorator (if there is an error it will be triggered on the first decorator).
                && !matches!(self.follows, Follows::Decorator)
                // The class's docstring can directly precede the first function.
                && !matches!(self.follows, Follows::Docstring)
                // Do not trigger when the def/class follows an "indenting token" (if/while/etc...).
                && prev_indent_level.is_some_and(|prev_indent_level| prev_indent_level >= line.indent_level)
                // Allow groups of one-liners.
                && !(matches!(self.follows, Follows::Def) && line.last_token != TokenKind::Colon)
            {
                // E306
                let mut diagnostic = Diagnostic::new(
                    BlankLinesBeforeNestedDefinition {
                        actual_blank_lines: line.blank_lines,
                    },
                    line.first_token_range,
                );

                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist.line_ending().as_str().to_string(),
                    locator.line_start(line.first_token_range.start()),
                )));

                diagnostics.push(diagnostic);
            }
        }

        match line.first_token {
            TokenKind::Class => {
                if matches!(self.class_status, Status::Outside) {
                    self.class_status = Status::Inside(line.indent_level);
                }
                self.follows = Follows::Other;
            }
            TokenKind::At => {
                self.follows = Follows::Decorator;
            }
            TokenKind::Def => {
                if matches!(self.fn_status, Status::Outside) {
                    self.fn_status = Status::Inside(line.indent_level);
                }
                self.follows = Follows::Def;
            }
            TokenKind::Comment => {}
            _ => {
                self.follows = Follows::Other;
            }
        }

        if !line.is_comment_only {
            if !self.is_not_first_logical_line {
                self.is_not_first_logical_line = true;
            }

            if line.is_docstring {
                self.follows = Follows::Docstring;
            }

            self.last_non_comment_line_end = line.last_token_range.end();

            if line.indent_level == 0 && line.first_token != TokenKind::Comment {
                self.previous_unindented_token = Some(line.first_token);
            }
        }
    }
}
