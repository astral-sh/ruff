use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_python_ast::source_code::Locator;

use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::token_kind::TokenKind;
use ruff_text_size::TextSize;

use crate::checkers::logical_lines::LogicalLinesContext;

use super::LogicalLine;

/// Contains variables used for the linting of blank lines.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct BlankLinesTrackingVars {
    /// Number of consecutive blank lines.
    blank_lines: u32,
    /// Number of blank characters in the blank lines (\n vs \r\n for example).
    blank_characters: u32,
    follows_decorator: bool,
    follows_def: bool,
    is_in_class: bool,
    /// The indent level where the class started.
    class_indent_level: usize,
    is_in_fn: bool,
    /// The indent level where the function started.
    fn_indent_level: usize,
}

/// Number of blank lines between various code parts.
struct BlankLinesConfig;

impl BlankLinesConfig {
    /// Number of blank lines around top level classes and functions.
    const TOP_LEVEL: u32 = 2;
    /// Number of blank lines around methods and nested classes and functions.
    const METHOD: u32 = 1;
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
pub struct BlankLineBetweenMethods;

impl AlwaysAutofixableViolation for BlankLineBetweenMethods {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected 1 blank line, found 0")
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for extraneous blank lines between top level functions and classes.
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
pub struct BlankLinesTopLevel(pub usize);

impl AlwaysAutofixableViolation for BlankLinesTopLevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesTopLevel(nb_blank_lines) = self;
        format!("Expected 2 blank lines, found {nb_blank_lines}")
    }

    fn autofix_title(&self) -> String {
        "Add missing blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for extraneous blank lines.
///
/// ## Why is this bad?
/// PEP 8 recommends the using blank lines as following:
/// - Two blank lines are expected between functions and classes
/// - One blank line is expected between methods of a class.
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

impl AlwaysAutofixableViolation for TooManyBlankLines {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBlankLines(nb_blank_lines) = self;
        format!("Too many blank lines ({nb_blank_lines})")
    }

    fn autofix_title(&self) -> String {
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

impl AlwaysAutofixableViolation for BlankLineAfterDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("blank lines found after function decorator")
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for blank lines after end of function or class.
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
pub struct BlankLinesAfterFunctionOrClass(pub usize);

impl AlwaysAutofixableViolation for BlankLinesAfterFunctionOrClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesAfterFunctionOrClass(blank_lines) = self;
        format!("expected 2 blank lines after class or function definition, found ({blank_lines})")
    }

    fn autofix_title(&self) -> String {
        "Add missing blank line(s)".to_string()
    }
}

/// ## What it does
/// Checks for blank lines after end of function or class.
///
/// ## Why is this bad?
/// PEP 8 recommends the using blank lines as following:
/// - Two blank lines are expected between functions and classes
/// - One blank line is expected between methods of a class.
///
/// ## Example  FIXME: The pycodestyle example does not trigger an error...
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

impl AlwaysAutofixableViolation for BlankLinesBeforeNestedDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLinesBeforeNestedDefinition(blank_lines) = self;
        format!("Expected 1 blank line before a nested definition, found {blank_lines}")
    }

    fn autofix_title(&self) -> String {
        "Add missing blank line".to_string()
    }
}

/// E301, E303
pub(crate) fn blank_lines(
    line: &LogicalLine,
    prev_line: Option<&LogicalLine>,
    tracked_vars: &mut BlankLinesTrackingVars,
    indent_level: usize,
    locator: &Locator,
    stylist: &Stylist,
    context: &mut LogicalLinesContext,
) {
    if line.is_empty() {
        tracked_vars.blank_lines += 1;
        tracked_vars.blank_characters += u32::try_from(line.text().len())
            .expect("The number of blank characters should be relatively small");
        return;
    }

    for (token_idx, token) in line.tokens().iter().enumerate() {
        // E301
        if token.kind() == TokenKind::Def
            && tracked_vars.is_in_class
            && tracked_vars.blank_lines == 0
            && prev_line
                .and_then(|prev_line| prev_line.tokens_trimmed().first())
                .map_or(false, |token| token.kind() != TokenKind::Class)
        {
            let mut diagnostic = Diagnostic::new(BlankLineBetweenMethods, token.range());
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::insertion(
                stylist.line_ending().as_str().to_string(),
                locator.line_start(token.range().start()),
            )));
            context.push_diagnostic(diagnostic);
        }

        // E302
        if token.kind() == TokenKind::Def
            && !tracked_vars.is_in_class
            && tracked_vars.blank_lines < 2
            && prev_line.is_some()
        {
            let mut diagnostic = Diagnostic::new(
                BlankLinesTopLevel(tracked_vars.blank_lines as usize),
                token.range(),
            );
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::insertion(
                stylist
                    .line_ending()
                    .as_str()
                    .to_string()
                    .repeat(2 - tracked_vars.blank_lines as usize),
                locator.line_start(token.range().start()),
            )));
            context.push_diagnostic(diagnostic);
        }

        // E303
        if token_idx == 0 && tracked_vars.blank_lines > BlankLinesConfig::TOP_LEVEL
            || (tracked_vars.is_in_class && tracked_vars.blank_lines > BlankLinesConfig::METHOD)
        {
            let mut diagnostic =
                Diagnostic::new(TooManyBlankLines(tracked_vars.blank_lines), token.range());

            let chars_to_remove = if indent_level > 0 {
                tracked_vars.blank_characters - BlankLinesConfig::METHOD
            } else {
                tracked_vars.blank_characters - BlankLinesConfig::TOP_LEVEL
            };
            let end = locator.line_start(token.range().start());
            let start = end - TextSize::new(chars_to_remove);
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::suggested(Edit::deletion(start, end)));

            context.push_diagnostic(diagnostic);
        }

        // E304
        if tracked_vars.follows_decorator && tracked_vars.blank_lines > 0 {
            let mut diagnostic = Diagnostic::new(BlankLineAfterDecorator, token.range());

            let range = token.range();
            diagnostic.set_fix(Fix::suggested(Edit::deletion(
                locator.line_start(range.start()) - TextSize::new(tracked_vars.blank_characters),
                locator.line_start(range.start()),
            )));
            context.push_diagnostic(diagnostic);
        }

        // E305
        if tracked_vars.blank_lines < 2
            && (tracked_vars.is_in_fn || tracked_vars.is_in_class)
            && indent_level == 0
        {
            let mut diagnostic = Diagnostic::new(
                BlankLinesAfterFunctionOrClass(tracked_vars.blank_lines as usize),
                token.range(),
            );
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::insertion(
                stylist
                    .line_ending()
                    .as_str()
                    .to_string()
                    .repeat(2 - tracked_vars.blank_lines as usize),
                locator.line_start(token.range().start()),
            )));
            context.push_diagnostic(diagnostic);
        }

        // E306
        if token.kind() == TokenKind::Def
            && tracked_vars.follows_def
            && tracked_vars.blank_lines == 0
        {
            let mut diagnostic = Diagnostic::new(
                BlankLinesBeforeNestedDefinition(tracked_vars.blank_lines),
                token.range(),
            );
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::insertion(
                stylist.line_ending().as_str().to_string(),
                locator.line_start(token.range().start()),
            )));

            context.push_diagnostic(diagnostic);
        }

        match token.kind() {
            TokenKind::Class => {
                if !tracked_vars.is_in_class {
                    tracked_vars.class_indent_level = indent_level;
                }
                tracked_vars.is_in_class = true;
                return;
            }
            TokenKind::At => {
                tracked_vars.follows_decorator = true;

                tracked_vars.follows_def = false;
                tracked_vars.blank_lines = 0;
                tracked_vars.blank_characters = 0;
                return;
            }
            TokenKind::Def => {
                if !tracked_vars.is_in_fn {
                    tracked_vars.fn_indent_level = indent_level;
                }
                tracked_vars.is_in_fn = true;
                tracked_vars.follows_def = true;

                tracked_vars.follows_decorator = false;
                tracked_vars.blank_lines = 0;
                tracked_vars.blank_characters = 0;
                return;
            }
            _ => {}
        }

        if indent_level <= tracked_vars.class_indent_level {
            tracked_vars.is_in_class = false;
        }

        if indent_level <= tracked_vars.fn_indent_level {
            tracked_vars.is_in_fn = false;
        }
    }
    tracked_vars.follows_decorator = false;
    tracked_vars.follows_def = false;
    tracked_vars.blank_lines = 0;
    tracked_vars.blank_characters = 0;
}
