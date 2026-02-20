use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;
use crate::checkers::ast::LintContext;
use crate::directives::{TodoComment, TodoDirectiveKind};

/// ## What it does
/// Checks for "TODO" comments.
///
/// ## Why is this bad?
/// "TODO" comments are used to describe an issue that should be resolved
/// (usually, a missing feature, optimization, or refactoring opportunity).
///
/// Consider resolving the issue before deploying the code.
///
/// Note that if you use "TODO" comments as a form of documentation (e.g.,
/// to [provide context for future work](https://gist.github.com/dmnd/ed5d8ef8de2e4cfea174bd5dafcda382)),
/// this rule may not be appropriate for your project.
///
/// ## Example
/// ```python
/// def greet(name):
///     return f"Hello, {name}!"  # TODO: Add support for custom greetings.
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.272")]
pub(crate) struct LineContainsTodo;
impl Violation for LineContainsTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Line contains TODO, consider resolving the issue".to_string()
    }
}

/// ## What it does
/// Checks for "FIXME" comments.
///
/// ## Why is this bad?
/// "FIXME" comments are used to describe an issue that should be resolved
/// (usually, a bug or unexpected behavior).
///
/// Consider resolving the issue before deploying the code.
///
/// Note that if you use "FIXME" comments as a form of documentation, this
/// rule may not be appropriate for your project.
///
/// ## Example
/// ```python
/// def speed(distance, time):
///     return distance / time  # FIXME: Raises ZeroDivisionError for time = 0.
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.272")]
pub(crate) struct LineContainsFixme;
impl Violation for LineContainsFixme {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Line contains FIXME, consider resolving the issue".to_string()
    }
}

/// ## What it does
/// Checks for "XXX" comments.
///
/// ## Why is this bad?
/// "XXX" comments are used to describe an issue that should be resolved.
///
/// Consider resolving the issue before deploying the code, or, at minimum,
/// using a more descriptive comment tag (e.g, "TODO").
///
/// ## Example
/// ```python
/// def speed(distance, time):
///     return distance / time  # XXX: Raises ZeroDivisionError for time = 0.
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.272")]
pub(crate) struct LineContainsXxx;
impl Violation for LineContainsXxx {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Line contains XXX, consider resolving the issue".to_string()
    }
}

/// ## What it does
/// Checks for "HACK" comments.
///
/// ## Why is this bad?
/// "HACK" comments are used to describe an issue that should be resolved
/// (usually, a suboptimal solution or temporary workaround).
///
/// Consider resolving the issue before deploying the code.
///
/// Note that if you use "HACK" comments as a form of documentation, this
/// rule may not be appropriate for your project.
///
/// ## Example
/// ```python
/// import os
///
///
/// def running_windows():  # HACK: Use platform module instead.
///     try:
///         os.mkdir("C:\\Windows\\System32\\")
///     except FileExistsError:
///         return True
///     else:
///         os.rmdir("C:\\Windows\\System32\\")
///         return False
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.272")]
pub(crate) struct LineContainsHack;
impl Violation for LineContainsHack {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Line contains HACK, consider resolving the issue".to_string()
    }
}

pub(crate) fn todos(context: &LintContext, directive_ranges: &[TodoComment]) {
    for TodoComment { directive, .. } in directive_ranges {
        match directive.kind {
            // FIX001
            TodoDirectiveKind::Fixme => {
                context.report_diagnostic_if_enabled(LineContainsFixme, directive.range);
            }
            // FIX002
            TodoDirectiveKind::Hack => {
                context.report_diagnostic_if_enabled(LineContainsHack, directive.range);
            }
            // FIX003
            TodoDirectiveKind::Todo => {
                context.report_diagnostic_if_enabled(LineContainsTodo, directive.range);
            }
            // FIX004
            TodoDirectiveKind::Xxx => {
                context.report_diagnostic_if_enabled(LineContainsXxx, directive.range);
            }
        }
    }
}
