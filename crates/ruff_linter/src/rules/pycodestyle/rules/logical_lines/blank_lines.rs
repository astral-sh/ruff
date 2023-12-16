use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use std::iter::Flatten;
use std::slice::Iter;

use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Stylist;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::lexer::LexicalError;
use ruff_python_parser::Tok;
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;

use ruff_text_size::TextRange;
use ruff_text_size::TextSize;

use crate::checkers::logical_lines::expand_indent;
use crate::checkers::logical_lines::LogicalLinesContext;

use super::LogicalLine;

/// Contains variables used for the linting of blank lines.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
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
    prev_indent_level: Option<usize>,
}

/// Number of blank lines around top level classes and functions.
const BLANK_LINES_TOP_LEVEL: u32 = 2;
/// Number of blank lines around methods and nested classes and functions.
const BLANK_LINES_METHOD_LEVEL: u32 = 1;

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
pub struct BlankLineBetweenMethods(pub u32);

impl AlwaysFixableViolation for BlankLineBetweenMethods {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineBetweenMethods(nb_blank_lines) = self;
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
pub struct BlankLinesTopLevel(pub u32);

impl AlwaysFixableViolation for BlankLinesTopLevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesTopLevel(nb_blank_lines) = self;
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
pub struct TooManyBlankLines(pub u32);

impl AlwaysFixableViolation for TooManyBlankLines {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBlankLines(nb_blank_lines) = self;
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
pub struct BlankLinesAfterFunctionOrClass(pub u32);

impl AlwaysFixableViolation for BlankLinesAfterFunctionOrClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesAfterFunctionOrClass(blank_lines) = self;
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
pub struct BlankLinesBeforeNestedDefinition(pub u32);

impl AlwaysFixableViolation for BlankLinesBeforeNestedDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesBeforeNestedDefinition(blank_lines) = self;
        format!("Expected 1 blank line before a nested definition, found {blank_lines}")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line".to_string()
    }
}

/// Returns `true` if line is a docstring only line.
fn is_docstring(line: &LogicalLine) -> bool {
    !line.tokens_trimmed().is_empty()
        && line
            .tokens_trimmed()
            .iter()
            .all(|token| matches!(token.kind(), TokenKind::String))
}

/// Returns `true` if the token is Async, Class or Def
fn is_top_level_token(token: Option<TokenKind>) -> bool {
    matches!(
        token,
        Some(TokenKind::Class | TokenKind::Def | TokenKind::Async)
    )
}

/// Returns `true` if the token is At, Async, Class or Def
fn is_top_level_token_or_decorator(token: TokenKind) -> bool {
    matches!(
        token,
        TokenKind::Class | TokenKind::Def | TokenKind::Async | TokenKind::At
    )
}

#[derive(Debug)]
struct LogicalLineInfo {
    first_token: Tok,
    line_is_comment_only: bool,
    indent_level: usize,
    blank_lines: u32,
    preceding_blank_lines: u32,
    preceding_blank_characters: TextSize,
    tokens_start: TextSize,
}

struct LinePreprocessor<'a> {
    tokens: Flatten<Iter<'a, Result<(Tok, TextRange), LexicalError>>>,
    locator: &'a Locator<'a>,
    /// Number of previous consecutive blank lines.
    previous_blank_lines: u32,
    /// Number of consecutive blank lines.
    current_blank_lines: u32,
    /// Number of blank characters in the blank lines (\n vs \r\n for example).
    current_blank_characters: TextSize,
}

impl<'a> LinePreprocessor<'a> {
    fn new(tokens: &'a [LexResult], locator: &'a Locator) -> LinePreprocessor<'a> {
        LinePreprocessor {
            tokens: tokens.into_iter().flatten(),
            locator,
            previous_blank_lines: 0,
            current_blank_lines: 0,
            current_blank_characters: 0.into(),
        }
    }
}

impl<'a> Iterator for LinePreprocessor<'a> {
    type Item = LogicalLineInfo;

    fn next(&mut self) -> Option<LogicalLineInfo> {
        let mut line_is_comment_only = true;
        let mut first_token: Option<Tok> = None;
        let mut first_token_range = TextRange::new(0.into(), 0.into());
        let mut parens = 0u32;

        let logical_line: LogicalLineInfo;

        while let Some((token, range)) = self.tokens.next() {
            if first_token.is_none() && !matches!(token, Tok::Indent | Tok::Dedent | Tok::Newline) {
                first_token = Some(token.clone());
                first_token_range = range.clone();
            }

            if !matches!(token, Tok::Comment(_)) {
                line_is_comment_only = false;
            }

            match token {
                Tok::Lbrace | Tok::Lpar | Tok::Lsqb => {
                    parens = parens.saturating_add(1);
                }
                Tok::Rbrace | Tok::Rpar | Tok::Rsqb => {
                    parens = parens.saturating_sub(1);
                }
                Tok::Newline | Tok::NonLogicalNewline if parens == 0 => {
                    let range = if matches!(first_token, Some(Tok::Indent)) {
                        first_token_range
                    } else {
                        TextRange::new(
                            self.locator.line_start(first_token_range.start()),
                            first_token_range.start(),
                        )
                    };
                    let indent_level = expand_indent(self.locator.slice(range));

                    // Empty line
                    if matches!(first_token, Some(Tok::NonLogicalNewline)) {
                        self.current_blank_lines += 1;
                        self.current_blank_characters += range.end() - first_token_range.start();
                        return self.next();
                    } else {
                        if self.previous_blank_lines < self.current_blank_lines {
                            self.previous_blank_lines = self.current_blank_lines;
                        }

                        logical_line = LogicalLineInfo {
                            first_token: first_token.clone().unwrap(),
                            line_is_comment_only, // TODO: Have an enum flag (to unify with docstring) ?
                            indent_level,
                            blank_lines: self.current_blank_lines,
                            preceding_blank_lines: self.previous_blank_lines,
                            preceding_blank_characters: self.current_blank_characters,
                            tokens_start: first_token_range.start(),
                        };

                        if line_is_comment_only {
                            self.previous_blank_lines = 0;
                        }
                        self.current_blank_lines = 0;
                        self.current_blank_characters = 0.into();
                        return Some(logical_line);
                    }
                }
                _ => {}
            }
        }

        return None;
    }
}

impl BlankLinesChecker {
    /// E301, E302, E303, E304, E305, E306
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_content(
        &mut self,
        tokens: &[LexResult],
        // line: &LogicalLine,
        //
        // indent_level: usize,
        indent_size: usize,
        locator: &Locator,
        stylist: &Stylist,
        context: &mut LogicalLinesContext,
    ) {
        let mut prev_indent_level: Option<usize> = None;
        let indent_size: usize = 4;
        let line_preprocessor = LinePreprocessor::new(tokens, locator);

        for logical_line in line_preprocessor {
            dbg!(logical_line);
            self.check_line(logical_line, prev_indent_level, locator, stylist, context);
            prev_indent_level = Some(logical_line.indent_level);
        }

        // TODO: have an is_docstring flag.
        // TODO: Need to keep last token for:
        // && !(matches!(self.follows, Follows::Def)
        //              && line
        //              .tokens_trimmed()
        //              .last()
        //              .map_or(false, |token| !matches!(token.kind(), TokenKind::Colon))
        //         )

        return;
    }

    pub(crate) fn check_line(
        &mut self,
        line: LogicalLineInfo,
        prev_indent_level: Option<usize>,
        locator: &Locator,
        stylist: &Stylist,
        context: &mut LogicalLinesContext,
    ) {
        if let Status::Inside(nesting_indent) = self.class_status {
            if indent_level < nesting_indent {
                if line_is_comment_only {
                    self.class_status = Status::CommentAfter(nesting_indent);
                } else {
                    self.class_status = Status::Outside;
                }
            }
        }

        if let Status::Inside(nesting_indent) = self.fn_status {
            if indent_level < nesting_indent {
                if line_is_comment_only {
                    self.fn_status = Status::CommentAfter(nesting_indent);
                } else {
                    self.fn_status = Status::Outside;
                }
            }
        }

        // A comment can be de-indented while still being in a class/function, in that case
        // we need to revert the variables.
        if !line_is_comment_only {
            if let Status::CommentAfter(indent) = self.fn_status {
                if indent_level >= indent {
                    self.fn_status = Status::Inside(indent);
                }
                self.fn_status = Status::Outside;
            }

            if let Status::CommentAfter(indent) = self.class_status {
                if indent_level >= indent {
                    self.class_status = Status::Inside(indent);
                }
                self.class_status = Status::Outside;
            }
        }

        // Don't expect blank lines before the first non comment line.
        if self.is_not_first_logical_line {
            if line.line.preceding_blank_lines == 0
                // Only applies to methods.
            && first_token.kind() == TokenKind::Def
                && matches!(self.class_status, Status::Inside(_))
                // The class/parent method's docstring can directly precede the def.
                && !matches!(self.follows, Follows::Docstring)
                // Do not trigger when the def follows an if/while/etc...
                && prev_indent_level.is_some_and(|prev_indent_level| prev_indent_level >= indent_level)
                // Allow following a decorator (if there is an error it will be triggered on the first decorator).
                && !matches!(self.follows, Follows::Decorator)
            {
                // E301
                let mut diagnostic = Diagnostic::new(
                    BlankLineBetweenMethods(line.line.preceding_blank_lines),
                    first_token.range,
                );
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist.line_ending().as_str().to_string(),
                    locator.line_start(self.last_non_comment_line_end),
                )));

                context.push_diagnostic(diagnostic);
            }

            if line.line.preceding_blank_lines < BLANK_LINES_TOP_LEVEL
                // Allow following a decorator (if there is an error it will be triggered on the first decorator).
                && !matches!(self.follows, Follows::Decorator)
                // Allow groups of one-liners.
                && !(matches!(self.follows, Follows::Def)
                    && line
                    .tokens_trimmed()
                    .last()
                    .map_or(false, |token| !matches!(token.kind(), TokenKind::Colon))
                )
                // Only trigger on non-indented classes and functions (for example functions within an if are ignored)
                && indent_level == 0
                // Only apply to functions or classes.
            && is_top_level_token_or_decorator(first_token.kind)
            {
                // E302
                let mut diagnostic = Diagnostic::new(
                    BlankLinesTopLevel(line.line.preceding_blank_lines),
                    first_token.range,
                );
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist
                        .line_ending()
                        .as_str()
                        .to_string()
                        .repeat((BLANK_LINES_TOP_LEVEL - line.line.preceding_blank_lines) as usize),
                    locator.line_start(self.last_non_comment_line_end),
                )));

                context.push_diagnostic(diagnostic);
            }

            if line.line.blank_lines > BLANK_LINES_TOP_LEVEL
                || (indent_level > 0 && line.line.blank_lines > BLANK_LINES_METHOD_LEVEL)
            {
                // E303
                let mut diagnostic =
                    Diagnostic::new(TooManyBlankLines(line.line.blank_lines), first_token.range);

                let chars_to_remove = if indent_level > 0 {
                    line.line.preceding_blank_characters - BLANK_LINES_METHOD_LEVEL
                } else {
                    line.line.preceding_blank_characters - BLANK_LINES_TOP_LEVEL
                };
                let end = locator.line_start(first_token.range.start());
                let start = end - TextSize::new(chars_to_remove);
                diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));

                context.push_diagnostic(diagnostic);
            }

            if matches!(self.follows, Follows::Decorator) && line.line.preceding_blank_lines > 0 {
                // E304
                let mut diagnostic = Diagnostic::new(BlankLineAfterDecorator, first_token.range);

                let range = first_token.range;
                diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                    locator.line_start(range.start())
                        - TextSize::new(line.line.preceding_blank_characters),
                    locator.line_start(range.start()),
                )));

                context.push_diagnostic(diagnostic);
            }

            if line.line.preceding_blank_lines < BLANK_LINES_TOP_LEVEL
                && is_top_level_token(self.previous_unindented_token)
                && indent_level == 0
                && !line_is_comment_only
                && !is_top_level_token_or_decorator(first_token.kind)
            {
                // E305
                let mut diagnostic = Diagnostic::new(
                    BlankLinesAfterFunctionOrClass(line.line.blank_lines),
                    first_token.range,
                );

                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist
                        .line_ending()
                        .as_str()
                        .to_string()
                        .repeat((BLANK_LINES_TOP_LEVEL - line.line.blank_lines) as usize),
                    locator.line_start(first_token.range.start()),
                )));

                context.push_diagnostic(diagnostic);
            }

            if line.line.preceding_blank_lines == 0
            // Only apply to nested functions.
                && matches!(self.fn_status, Status::Inside(_))
                && is_top_level_token_or_decorator(first_token.kind)
                // Allow following a decorator (if there is an error it will be triggered on the first decorator).
                && !matches!(self.follows, Follows::Decorator)
                // The class's docstring can directly precede the first function.
                && !matches!(self.follows, Follows::Docstring)
                // Do not trigger when the def/class follows an "indenting token" (if/while/etc...).
                && prev_indent_level.is_some_and(|prev_indent_level| prev_indent_level >= indent_level)
                // Allow groups of one-liners.
                && !(matches!(self.follows, Follows::Def)
                     && line
                     .tokens_trimmed()
                     .last()
                     .map_or(false, |token| !matches!(token.kind(), TokenKind::Colon))
                )
            {
                // E306
                let mut diagnostic = Diagnostic::new(
                    BlankLinesBeforeNestedDefinition(line.line.blank_lines),
                    first_token.range,
                );

                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                    stylist.line_ending().as_str().to_string(),
                    locator.line_start(first_token.range.start()),
                )));

                context.push_diagnostic(diagnostic);
            }
        }

        match first_token.kind() {
            TokenKind::Class => {
                if matches!(self.class_status, Status::Outside) {
                    self.class_status = Status::Inside(indent_level + indent_size);
                }
                self.follows = Follows::Other;
            }
            TokenKind::At => {
                self.follows = Follows::Decorator;
            }
            TokenKind::Def | TokenKind::Async => {
                if matches!(self.fn_status, Status::Outside) {
                    self.fn_status = Status::Inside(indent_level + indent_size);
                }
                self.follows = Follows::Def;
            }
            TokenKind::Comment => {}
            _ => {
                self.follows = Follows::Other;
            }
        }

        if !line_is_comment_only {
            if !self.is_not_first_logical_line {
                self.is_not_first_logical_line = true;
            }

            if is_docstring(line) {
                self.follows = Follows::Docstring;
            }

            self.last_non_comment_line_end = line
                .tokens()
                .last()
                .expect("Line to contain at least one token.")
                .range
                .end();

            if indent_level == 0 && !line.tokens_trimmed().is_empty() {
                self.previous_unindented_token = Some(
                    line.tokens_trimmed()
                        .first()
                        .expect("Previously checked.")
                        .kind,
                );
            }
        }
    }
}
