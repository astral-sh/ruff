use ruff_python_ast as ast;
use ruff_python_ast::{Arguments, Expr, StmtClassDef};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::Stmt;

use crate::checkers::ast::Checker;
use crate::rules::flake8_slots::rules::helpers::has_slots;

/// ## What it does
/// Checks for subclasses of `collections.namedtuple` that lack a `__slots__`
/// definition.
///
/// ## Why is this bad?
/// In Python, the `__slots__` attribute allows you to explicitly define the
/// attributes (instance variables) that a class can have. By default, Python
/// uses a dictionary to store an object's attributes, which incurs some memory
/// overhead. However, when `__slots__` is defined, Python uses a more compact
/// internal structure to store the object's attributes, resulting in memory
/// savings.
///
/// Subclasses of `namedtuple` inherit all the attributes and methods of the
/// built-in `namedtuple` class. Since tuples are typically immutable, they
/// don't require additional attributes beyond what the `namedtuple` class
/// provides. Defining `__slots__` for subclasses of `namedtuple` prevents the
/// creation of a dictionary for each instance, reducing memory consumption.
///
/// ## Example
/// ```python
/// from collections import namedtuple
///
///
/// class Foo(namedtuple("foo", ["str", "int"])):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// from collections import namedtuple
///
///
/// class Foo(namedtuple("foo", ["str", "int"])):
///     __slots__ = ()
/// ```
///
/// ## References
/// - [Python documentation: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
#[violation]
pub struct NoSlotsInNamedtupleSubclass;

impl Violation for NoSlotsInNamedtupleSubclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Subclasses of `collections.namedtuple()` should define `__slots__`")
    }
}

/// SLOT002
pub(crate) fn no_slots_in_namedtuple_subclass(
    checker: &mut Checker,
    stmt: &Stmt,
    class: &StmtClassDef,
) {
    let Some(Arguments { args: bases, .. }) = class.arguments.as_deref() else {
        return;
    };

    if bases.iter().any(|base| {
        let Expr::Call(ast::ExprCall { func, .. }) = base else {
            return false;
        };
        checker
            .semantic()
            .resolve_call_path(func)
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["collections", "namedtuple"]))
    }) {
        if !has_slots(&class.body) {
            checker.diagnostics.push(Diagnostic::new(
                NoSlotsInNamedtupleSubclass,
                stmt.identifier(),
            ));
        }
    }
}
