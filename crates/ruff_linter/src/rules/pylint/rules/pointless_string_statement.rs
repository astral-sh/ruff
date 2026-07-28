use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_semantic::ScopeKind;
use ruff_python_semantic::analyze::visibility;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::{Checker, DocstringState, ExpectedDocstringKind};
use crate::rules::flake8_bugbear::helpers::at_last_top_level_expression_in_cell;

/// ## What it does
/// Checks for string literals that appear as standalone statements without
/// serving as a docstring.
///
/// ## Why is this bad?
/// A string statement that is not a docstring has no effect at runtime. It is
/// usually a misplaced docstring, a comment written as a string, or leftover
/// debugging text.
///
/// Two placements are recognized as documentation and allowed:
///
/// - A string as the first statement of a module, class, or function body
///   (a docstring, per [PEP 257]).
/// - A string immediately following an assignment at the module, class, or
///   `__init__` level (an "attribute docstring", per [PEP 257]).
///
/// Matching pylint, a string immediately following a docstring (an
/// "additional docstring" in [PEP 257]'s terms) is still flagged.
///
/// The rule offers no fix: the correct remediation, whether moving the
/// string to a docstring position, converting it to a `#` comment, or
/// deleting it, depends on the author's intent.
///
/// ## Example
/// ```python
/// def foo():
///     x = 1
///     "This string has no effect."
///     return x
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     x = 1
///     # This comment documents the code.
///     return x
/// ```
///
/// ## Notebook behavior
/// For Jupyter Notebooks, this rule is not applied to a string that is the
/// last top-level expression in a cell, since the `repr` of the evaluated
/// expression is printed as the cell's output.
///
/// ## References
/// - [Pylint documentation: `pointless-string-statement`](https://pylint.readthedocs.io/en/stable/user_guide/messages/warning/pointless-string-statement.html)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct PointlessStringStatement;

impl Violation for PointlessStringStatement {
    #[derive_message_formats]
    fn message(&self) -> String {
        "String statement has no effect".to_string()
    }
}

/// PLW0105
pub(crate) fn pointless_string_statement(checker: &Checker, suite: &[Stmt]) {
    // Whether this suite is the body of the enclosing module, class, or
    // function (as opposed to a nested block like `if` or `for`, which has no
    // docstring concept).
    let is_scope_body = matches!(
        checker.docstring_state(),
        DocstringState::Expected(
            ExpectedDocstringKind::Module
                | ExpectedDocstringKind::Class
                | ExpectedDocstringKind::Function
        )
    );

    let scope_kind = &checker.semantic().current_scope().kind;

    // Attribute docstrings only exist at the module, class, or `__init__`
    // level. Other function bodies have no attribute docstring concept.
    let scope_allows_attribute_docstrings = match scope_kind {
        ScopeKind::Module | ScopeKind::Class(_) => true,
        ScopeKind::Function(function_def) => visibility::is_init(&function_def.name),
        _ => false,
    };

    // Whether the statements in this suite are top-level statements of a
    // notebook.
    let is_notebook_top_level =
        checker.source_type.is_ipynb() && is_scope_body && matches!(scope_kind, ScopeKind::Module);

    for (index, stmt) in suite.iter().enumerate() {
        if !is_docstring_stmt(stmt) {
            continue;
        }

        // The first statement of a module, class, or function body is the
        // real docstring.
        if is_scope_body && index == 0 {
            continue;
        }

        // An "attribute docstring" is a string immediately following an
        // assignment.
        if scope_allows_attribute_docstrings
            && index > 0
            && matches!(
                &suite[index - 1],
                Stmt::Assign(_) | Stmt::AnnAssign(_) | Stmt::TypeAlias(_)
            )
        {
            continue;
        }

        // In a notebook, a string that is a cell's last expression is the
        // cell's displayed output, not a pointless statement.
        if is_notebook_top_level
            && at_last_top_level_expression_in_cell(
                stmt.end(),
                checker.locator(),
                checker.cell_offsets(),
            )
        {
            continue;
        }

        checker.report_diagnostic(PointlessStringStatement, stmt.range());
    }
}
