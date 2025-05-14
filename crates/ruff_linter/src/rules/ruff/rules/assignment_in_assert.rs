use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::Binding;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for named assignment expressions (e.g., `x := 0`) in `assert`
/// statements.
///
/// ## Why is this bad?
/// Named assignment expressions (also known as "walrus operators") are used to
/// assign a value to a variable as part of a larger expression.
///
/// Named assignments are syntactically valid in `assert` statements. However,
/// when the Python interpreter is run under the `-O` flag, `assert` statements
/// are not executed. In this case, the named assignment will also be ignored,
/// which may result in unexpected behavior (e.g., undefined variable
/// accesses).
///
/// ## Example
/// ```python
/// assert (x := 0) == 0
/// print(x)
/// ```
///
/// Use instead:
/// ```python
/// x = 0
/// assert x == 0
/// print(x)
/// ```
///
/// The rule avoids flagging named expressions that define variables which are
/// only referenced from inside `assert` statements; the following will not
/// trigger the rule:
/// ```python
/// assert (x := y**2) > 42, f"Expected >42 but got {x}"
/// ```
///
/// Nor will this:
/// ```python
/// assert (x := y**2) > 42
/// assert x < 1_000_000
/// ```
///
/// ## References
/// - [Python documentation: `-O`](https://docs.python.org/3/using/cmdline.html#cmdoption-O)
#[derive(ViolationMetadata)]
pub(crate) struct AssignmentInAssert;

impl Violation for AssignmentInAssert {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid assignment expressions in `assert` statements".to_string()
    }
}

/// RUF018
pub(crate) fn assignment_in_assert(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    if !binding.in_assert_statement() {
        return None;
    }

    let semantic = checker.semantic();

    let parent_expression = binding.expression(semantic)?.as_named_expr()?;

    if binding
        .references()
        .all(|reference| semantic.reference(reference).in_assert_statement())
    {
        return None;
    }

    Some(Diagnostic::new(
        AssignmentInAssert,
        parent_expression.range(),
    ))
}
