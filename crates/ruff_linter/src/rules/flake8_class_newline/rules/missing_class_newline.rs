use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Stylist;
use ruff_python_parser::lexer::LexResult;
use ruff_source_file::Locator;

use crate::line_width::IndentWidth;
use crate::rules::pycodestyle::rules::{LinePreprocessor, LogicalLineInfo, LogicalLineKind};

/// ## What it does
/// Checks for a missing blank line between a class definition and its first method.
///
/// ## Why is this bad?
/// PEP8 says we should surround every class method with a single blank line.
/// However it is ambiguous about the first method having a blank line above it.
/// This rule enforces a single blank line, for consistency across classes.
///
/// ## Example
/// ```python
/// class MyClass(object):
///     def func1():
///         pass
/// ```
///
/// Use instead:
/// ```python
/// class MyClass(object):
///
///     def func1():
///         pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#blank-lines)
#[violation]
pub struct MissingClassNewLine;

impl AlwaysFixableViolation for MissingClassNewLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected 1 blank line after class declaration, found 0")
    }

    fn fix_title(&self) -> String {
        "Add missing blank line".to_string()
    }
}

#[derive(Copy, Clone, Debug, Default)]
enum Follows {
    #[default]
    Class,
    Other,
}

/// Contains variables used for the linting of blank lines.
#[derive(Debug, Default)]
pub(crate) struct BlankLinesChecker {
    follows: Follows,
}

impl BlankLinesChecker {
    pub(crate) fn check_lines(
        &mut self,
        tokens: &[LexResult],
        locator: &Locator,
        stylist: &Stylist,
        indent_width: IndentWidth,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let line_preprocessor = LinePreprocessor::new(tokens, locator, indent_width);

        for logical_line in line_preprocessor {
            self.check_new_line_after_class_declaration(
                &logical_line,
                locator,
                stylist,
                diagnostics,
            );
        }
    }

    fn check_new_line_after_class_declaration(
        &mut self,
        line: &LogicalLineInfo,
        locator: &Locator,
        stylist: &Stylist,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if (matches!(self.follows, Follows::Class)
            && matches!(
                line.kind,
                LogicalLineKind::Function | LogicalLineKind::Decorator
            )
            && line.preceding_blank_lines == 0)
        {
            let mut diagnostic = Diagnostic::new(MissingClassNewLine, line.first_token_range);
            diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                stylist.line_ending().to_string(),
                locator.line_start(line.first_token_range.start()),
            )));

            diagnostics.push(diagnostic);
        }

        // Update the `self.follows` state based on the current line
        match line.kind {
            LogicalLineKind::Class => self.follows = Follows::Class,
            _ => self.follows = Follows::Other,
        }
    }
}
