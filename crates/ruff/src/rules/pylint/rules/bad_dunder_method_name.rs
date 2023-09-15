use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::Stmt;
use ruff_python_semantic::analyze::visibility;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for misspelled and unknown dunder names in method definitions.
///
/// ## Why is this bad?
/// Misspelled dunder name methods may cause your code to not function
/// as expected.
///
/// Since dunder methods are associated with customizing the behavior
/// of a class in Python, introducing a dunder method such as `__foo__`
/// that diverges from standard Python dunder methods could potentially
/// confuse someone reading the code.
///
/// This rule will detect all methods starting and ending with at least
/// one underscore (e.g., `_str_`), but ignores known dunder methods (like
/// `__init__`), as well as methods that are marked with `@override`.
///
/// ## Example
/// ```python
/// class Foo:
///     def __init_(self):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __init__(self):
///         ...
/// ```
#[violation]
pub struct BadDunderMethodName {
    name: String,
}

impl Violation for BadDunderMethodName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadDunderMethodName { name } = self;
        format!("Bad or misspelled dunder method name `{name}`. (bad-dunder-name)")
    }
}

/// PLW3201
pub(crate) fn bad_dunder_method_name(checker: &mut Checker, class_body: &[Stmt]) {
    for method in class_body
        .iter()
        .filter_map(ruff_python_ast::Stmt::as_function_def_stmt)
        .filter(|method| {
            if is_known_dunder_method(&method.name) {
                return false;
            }
            method.name.starts_with('_') && method.name.ends_with('_')
        })
    {
        if visibility::is_override(&method.decorator_list, checker.semantic()) {
            continue;
        }
        checker.diagnostics.push(Diagnostic::new(
            BadDunderMethodName {
                name: method.name.to_string(),
            },
            method.identifier(),
        ));
    }
}

/// Returns `true` if a method is a known dunder method.
fn is_known_dunder_method(method: &str) -> bool {
    matches!(
        method,
        "__abs__"
            | "__add__"
            | "__aenter__"
            | "__aexit__"
            | "__aiter__"
            | "__and__"
            | "__anext__"
            | "__await__"
            | "__bool__"
            | "__bytes__"
            | "__call__"
            | "__ceil__"
            | "__class__"
            | "__class_getitem__"
            | "__complex__"
            | "__contains__"
            | "__copy__"
            | "__deepcopy__"
            | "__del__"
            | "__delattr__"
            | "__delete__"
            | "__delitem__"
            | "__dict__"
            | "__dir__"
            | "__divmod__"
            | "__doc__"
            | "__enter__"
            | "__eq__"
            | "__exit__"
            | "__float__"
            | "__floor__"
            | "__floordiv__"
            | "__format__"
            | "__fspath__"
            | "__ge__"
            | "__get__"
            | "__getattr__"
            | "__getattribute__"
            | "__getitem__"
            | "__getnewargs__"
            | "__getnewargs_ex__"
            | "__getstate__"
            | "__gt__"
            | "__hash__"
            | "__iadd__"
            | "__iand__"
            | "__ifloordiv__"
            | "__ilshift__"
            | "__imatmul__"
            | "__imod__"
            | "__imul__"
            | "__init__"
            | "__init_subclass__"
            | "__instancecheck__"
            | "__int__"
            | "__invert__"
            | "__ior__"
            | "__ipow__"
            | "__irshift__"
            | "__isub__"
            | "__iter__"
            | "__itruediv__"
            | "__ixor__"
            | "__le__"
            | "__len__"
            | "__length_hint__"
            | "__lshift__"
            | "__lt__"
            | "__matmul__"
            | "__missing__"
            | "__mod__"
            | "__module__"
            | "__mul__"
            | "__ne__"
            | "__neg__"
            | "__new__"
            | "__next__"
            | "__or__"
            | "__pos__"
            | "__post_init__"
            | "__pow__"
            | "__radd__"
            | "__rand__"
            | "__rdivmod__"
            | "__reduce__"
            | "__reduce_ex__"
            | "__repr__"
            | "__reversed__"
            | "__rfloordiv__"
            | "__rlshift__"
            | "__rmatmul__"
            | "__rmod__"
            | "__rmul__"
            | "__ror__"
            | "__round__"
            | "__rpow__"
            | "__rrshift__"
            | "__rshift__"
            | "__rsub__"
            | "__rtruediv__"
            | "__rxor__"
            | "__set__"
            | "__set_name__"
            | "__setattr__"
            | "__setitem__"
            | "__setstate__"
            | "__sizeof__"
            | "__str__"
            | "__sub__"
            | "__subclasscheck__"
            | "__subclasses__"
            | "__subclasshook__"
            | "__truediv__"
            | "__trunc__"
            | "__weakref__"
            | "__xor__"
    )
}
