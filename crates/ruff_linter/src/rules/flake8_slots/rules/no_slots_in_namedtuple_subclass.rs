use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, identifier::Identifier, Arguments, Expr, Stmt, StmtClassDef};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::rules::flake8_slots::rules::helpers::has_slots;

/// ## What it does
/// Checks for subclasses of `collections.namedtuple` or `typing.NamedTuple`
/// that lack a `__slots__` definition.
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
pub struct NoSlotsInNamedtupleSubclass(NamedTupleKind);

impl Violation for NoSlotsInNamedtupleSubclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoSlotsInNamedtupleSubclass(namedtuple_kind) = self;
        format!("Subclasses of {namedtuple_kind} should define `__slots__`")
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum NamedTupleKind {
    Collections,
    Typing,
}

impl fmt::Display for NamedTupleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Collections => "`collections.namedtuple()`",
            Self::Typing => "call-based `typing.NamedTuple()`",
        })
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

    if let Some(namedtuple_kind) = namedtuple_base(bases, checker.semantic()) {
        if !has_slots(&class.body) {
            checker.diagnostics.push(Diagnostic::new(
                NoSlotsInNamedtupleSubclass(namedtuple_kind),
                stmt.identifier(),
            ));
        }
    }
}

/// If the class has a call-based namedtuple in its bases,
/// return the kind of namedtuple it is
/// (either `collections.namedtuple()`, or `typing.NamedTuple()`).
/// Else, return `None`.
fn namedtuple_base(bases: &[Expr], semantic: &SemanticModel) -> Option<NamedTupleKind> {
    for base in bases {
        let Expr::Call(ast::ExprCall { func, .. }) = base else {
            continue;
        };
        let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
            continue;
        };
        match qualified_name.segments() {
            ["collections", "namedtuple"] => return Some(NamedTupleKind::Collections),
            ["typing", "NamedTuple"] => return Some(NamedTupleKind::Typing),
            _ => continue,
        }
    }
    None
}
