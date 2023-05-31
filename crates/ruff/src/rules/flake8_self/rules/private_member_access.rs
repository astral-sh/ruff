use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for accesses on "private" class members.
///
/// ## Why is this bad?
/// In Python, the convention is such that class members that are prefixed
/// with a single underscore, or prefixed but not suffixed with a double
/// underscore, are considered private and intended for internal use.
///
/// Using such "private" members is considered a misuse of the class, as
/// there are no guarantees that the member will be present in future
/// versions, that it will have the same type, or that it will have the same
/// behavior. Instead, use the class's public interface.
///
/// ## Options
/// - `flake8-self.ignore-names`
///
/// ## Example
/// ```python
/// class Class:
///     def __init__(self):
///         self._private_member = "..."
///
///
/// var = Class()
/// print(var._private_member)
/// ```
///
/// Use instead:
/// ```python
/// class Class:
///     def __init__(self):
///         self.public_member = "..."
///
///
/// var = Class()
/// print(var.public_member)
/// ```
///
/// ## References
/// - [_What is the meaning of single or double underscores before an object name?_](https://stackoverflow.com/questions/1301346/what-is-the-meaning-of-single-and-double-underscore-before-an-object-name)
#[violation]
pub struct PrivateMemberAccess {
    access: String,
}

impl Violation for PrivateMemberAccess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PrivateMemberAccess { access } = self;
        format!("Private member accessed: `{access}`")
    }
}

/// SLF001
pub(crate) fn private_member_access(checker: &mut Checker, expr: &Expr) {
    if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = expr {
        if (attr.starts_with("__") && !attr.ends_with("__"))
            || (attr.starts_with('_') && !attr.starts_with("__"))
        {
            if checker
                .settings
                .flake8_self
                .ignore_names
                .contains(attr.as_ref())
            {
                return;
            }

            if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
                // Ignore `super()` calls.
                if let Some(call_path) = collect_call_path(func) {
                    if call_path.as_slice() == ["super"] {
                        return;
                    }
                }
            } else {
                // Ignore `self` and `cls` accesses.
                if let Some(call_path) = collect_call_path(value) {
                    if call_path.as_slice() == ["self"]
                        || call_path.as_slice() == ["cls"]
                        || call_path.as_slice() == ["mcs"]
                    {
                        return;
                    }

                    // Ignore accesses on class members from _within_ the class.
                    if checker
                        .semantic_model()
                        .scopes
                        .iter()
                        .rev()
                        .find_map(|scope| match &scope.kind {
                            ScopeKind::Class(ast::StmtClassDef { name, .. }) => Some(name),
                            _ => None,
                        })
                        .map_or(false, |name| {
                            if call_path.as_slice() == [name.as_str()] {
                                checker.semantic_model().find_binding(name).map_or(
                                    false,
                                    |binding| {
                                        // TODO(charlie): Could the name ever be bound to a
                                        // _different_ class here?
                                        binding.kind.is_class_definition()
                                    },
                                )
                            } else {
                                false
                            }
                        })
                    {
                        return;
                    }
                }
            }

            checker.diagnostics.push(Diagnostic::new(
                PrivateMemberAccess {
                    access: attr.to_string(),
                },
                expr.range(),
            ));
        }
    }
}
