use ruff_python_ast::{ExprKind, NodeList};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for assignments to `os.environ`.
///
/// ## Why is this bad?
/// In Python, `os.environ` is a mapping that represents the environment of the
/// current process.
///
/// However, reassigning to `os.environ` does not clear the environment. Instead,
/// it merely updates the `os.environ` for the current process. This can lead to
/// unexpected behavior, especially when running the program in a subprocess.
///
/// Instead, use `os.environ.clear()` to clear the environment, or use the
/// `env` argument of `subprocess.Popen` to pass a custom environment to
/// a subprocess.
///
/// ## Example
/// ```python
/// import os
///
/// os.environ = {"foo": "bar"}
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// os.environ.clear()
/// os.environ["foo"] = "bar"
/// ```
///
/// ## References
/// - [Python documentation: `os.environ`](https://docs.python.org/3/library/os.html#os.environ)
/// - [Python documentation: `subprocess.Popen`](https://docs.python.org/3/library/subprocess.html#subprocess.Popen)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.102", category = Category::Suspicious)]
pub(crate) struct AssignmentToOsEnviron;

impl Violation for AssignmentToOsEnviron {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Assigning to `os.environ` doesn't clear the environment".to_string()
    }
}

/// B003
pub(crate) fn assignment_to_os_environ(
    checker: &Checker,
    targets: NodeList<'_, ruff_python_ast::kind::Expr>,
) {
    let Some([target]) = targets.as_array() else {
        return;
    };
    let ExprKind::Attribute(attribute) = target.classify() else {
        return;
    };
    let value = attribute.value();
    let attr = attribute.attr();
    if attr != "environ" {
        return;
    }
    let ExprKind::Name(name) = value.classify() else {
        return;
    };
    let id = name.id();
    if id != "os" {
        return;
    }
    checker.report_diagnostic(AssignmentToOsEnviron, target.range());
}
