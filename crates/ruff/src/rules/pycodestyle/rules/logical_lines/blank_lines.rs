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

/// Number of blank lines between various code parts.
struct BlankLinesConfig;

impl BlankLinesConfig {
    /// Top level class and function.
    const TOP_LEVEL: u32 = 2;
    /// Methods and nested class and function.
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
    blank_lines: &mut u32,
    blank_characters: &mut u32,
    follows_decorator: &mut bool,
    follows_def: &mut bool,
    is_in_class: &mut bool,
    class_indent_level: &mut usize,
    is_in_fn: &mut bool,
    fn_indent_level: &mut usize,
    indent_level: usize,
    locator: &Locator,
    stylist: &Stylist,
    context: &mut LogicalLinesContext,
) {
    if line.is_empty() {
        *blank_lines += 1;
        *blank_characters += line.text().len() as u32;
        return;
    }

    for (token_idx, token) in line.tokens().iter().enumerate() {
        // E304
        if *follows_decorator && *blank_lines > 0 {
            let mut diagnostic = Diagnostic::new(BlankLineAfterDecorator, token.range());

            let range = token.range();
            diagnostic.set_fix(Fix::suggested(Edit::deletion(
                locator.line_start(range.start())
                    - TextSize::new(
                        u32::try_from(*blank_characters)
                            .expect("The number of blank characters should be relatively small"),
                    ),
                locator.line_start(range.start()),
            )));
            context.push_diagnostic(diagnostic);
        }
        // E303
        else if token_idx == 0 && *blank_lines > BlankLinesConfig::TOP_LEVEL
            || (*is_in_class && *blank_lines > BlankLinesConfig::METHOD)
        {
            let mut diagnostic = Diagnostic::new(TooManyBlankLines(*blank_lines), token.range());

            let chars_to_remove = if indent_level > 0 {
                *blank_characters - BlankLinesConfig::METHOD
            } else {
                *blank_characters - BlankLinesConfig::TOP_LEVEL
            };
            let end = locator.line_start(token.range().start());
            let start = end - TextSize::new(chars_to_remove);
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::suggested(Edit::deletion(start, end)));

            context.push_diagnostic(diagnostic);
        }
        // E306
        else if token.kind() == TokenKind::Def && *follows_def && *blank_lines == 0 {
            let mut diagnostic = Diagnostic::new(
                BlankLinesBeforeNestedDefinition(*blank_lines),
                token.range(),
            );
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::insertion(
                stylist.line_ending().as_str().to_string(),
                locator.line_start(token.range().start()),
            )));

            context.push_diagnostic(diagnostic);
        }
        // E301
        if token.kind() == TokenKind::Def
            && *is_in_class
            && *blank_lines == 0
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
            && !*is_in_class
            && *blank_lines < 2
            && prev_line.is_some()
        {
            let mut diagnostic =
                Diagnostic::new(BlankLinesTopLevel(*blank_lines as usize), token.range());
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::insertion(
                stylist
                    .line_ending()
                    .as_str()
                    .to_string()
                    .repeat(2 - *blank_lines as usize),
                locator.line_start(token.range().start()),
            )));
            context.push_diagnostic(diagnostic);
        }
        // E305
        if *blank_lines < 2
            && ((*is_in_class && indent_level == 0) || (*is_in_fn && indent_level == 0))
        {
            let mut diagnostic = Diagnostic::new(
                BlankLinesAfterFunctionOrClass(*blank_lines as usize),
                token.range(),
            );
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::insertion(
                stylist
                    .line_ending()
                    .as_str()
                    .to_string()
                    .repeat(2 - *blank_lines as usize),
                locator.line_start(token.range().start()),
            )));
            context.push_diagnostic(diagnostic);
        }

        match token.kind() {
            TokenKind::Class => {
                if !*is_in_class {
                    *class_indent_level = indent_level;
                }
                *is_in_class = true;
                return;
            }
            TokenKind::At => {
                *follows_decorator = true;

                *follows_def = false;
                *blank_lines = 0;
                *blank_characters = 0;
                return;
            }
            TokenKind::Def => {
                if !*is_in_fn {
                    *fn_indent_level = indent_level;
                }
                *is_in_fn = true;
                *follows_def = true;

                *follows_decorator = false;
                *blank_lines = 0;
                *blank_characters = 0;
                return;
            }
            _ => {}
        }

        if indent_level <= *class_indent_level {
            *is_in_class = false;
        }

        if indent_level <= *fn_indent_level {
            *is_in_fn = false;
        }
    }
    *follows_decorator = false;
    *follows_def = false;
    *blank_lines = 0;
    *blank_characters = 0;
}
