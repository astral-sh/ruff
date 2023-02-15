use rustpython_parser::ast::{Expr, ExprKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::helpers::collect_call_path;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for the access of a private member of a class.
    ///
    /// ## Why is this bad?
    /// If a member of a class is declared private, the standard is that
    /// the member in question generally shouldn't be accessed by anything
    /// except for internally in the class. Using private variables can
    /// also possibly cause problems if used incorrectly, and those errors
    /// can be difficult to debug.
    ///
    /// Instead, the name of the member should be renamed to public
    /// (no leading underscores) if possible. If this is not possible,
    /// consider removing the usage or adding a `noqa` statement.
    ///
    /// ## Example
    /// ```python
    /// class MyClass:
    ///     def __init__(self):
    ///         self._private_member = "this is only supposed to be used internally"
    ///
    /// var_myclass = MyClass()
    /// print(var_myclass._private_member)
    /// ```
    ///
    /// Instead, use
    /// ```python
    /// class MyClass:
    ///     def __init__(self):
    ///         self.public_member = "public (underscore prefix is removed)"
    ///
    /// var_myclass = MyClass()
    /// print(var_myclass.public_member)
    /// ```
    ///
    /// * [What is the meaning of single or double underscores before an object name?_](https://stackoverflow.com/questions/1301346/what-is-the-meaning-of-single-and-double-underscore-before-an-object-name)
    /// ```
    pub struct PrivateMemberAccess {
        pub access: String,
    }
);
impl Violation for PrivateMemberAccess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PrivateMemberAccess { access } = self;
        format!("Private member accessed: `{access}`")
    }
}

/// SLF001
pub fn private_member_access(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Attribute { value, attr, .. } = &expr.node {
        if !attr.ends_with("__") && (attr.starts_with('_') || attr.starts_with("__")) {
            if let ExprKind::Call { func, .. } = &value.node {
                let call_path = collect_call_path(func);
                if call_path.as_slice() == ["super"] {
                    return;
                }
            } else {
                let call_path = collect_call_path(value);
                if call_path.as_slice() == ["self"]
                    || call_path.as_slice() == ["cls"]
                    || call_path.as_slice() == ["mcs"]
                {
                    return;
                }
            }

            checker.diagnostics.push(Diagnostic::new(
                PrivateMemberAccess {
                    access: attr.to_string(),
                },
                Range::from_located(expr),
            ));
        }
    }
}
