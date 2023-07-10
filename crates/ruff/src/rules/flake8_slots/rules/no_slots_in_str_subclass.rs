use rustpython_parser::ast::{Stmt, StmtClassDef};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;

use crate::checkers::ast::Checker;
use crate::rules::flake8_slots::rules::helpers::has_slots;

/// ## What it does
/// Checks for subclasses of `str` that lack a `__slots__` definition.
///
/// ## Why is this bad?
/// In Python, the `__slots__` attribute allows you to explicitly define the
/// attributes (instance variables) that a class can have. By default, Python
/// uses a dictionary to store an object's attributes, which incurs some memory
/// overhead. However, when `__slots__` is defined, Python uses a more compact
/// internal structure to store the object's attributes, resulting in memory
/// savings.
///
/// Subclasses of `str` inherit all the attributes and methods of the built-in
/// `str` class. Since strings are typically immutable, they don't require
/// additional attributes beyond what the `str` class provides. Defining
/// `__slots__` for subclasses of `str` prevents the creation of a dictionary
/// for each instance, reducing memory consumption.
///
/// ## Example
/// ```python
/// class Foo(str):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// class Foo(str):
///     __slots__ = ()
/// ```
///
/// ## References
/// - [Python documentation: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
#[violation]
pub struct NoSlotsInStrSubclass;

impl Violation for NoSlotsInStrSubclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Subclasses of `str` should define `__slots__`")
    }
}

/// SLOT000
pub(crate) fn no_slots_in_str_subclass(checker: &mut Checker, stmt: &Stmt, class: &StmtClassDef) {
    if class.bases.iter().any(|base| {
        checker
            .semantic()
            .resolve_call_path(base)
            .map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["" | "builtins", "str"])
            })
    }) {
        if !has_slots(&class.body) {
            checker
                .diagnostics
                .push(Diagnostic::new(NoSlotsInStrSubclass, stmt.identifier()));
        }
    }
}
