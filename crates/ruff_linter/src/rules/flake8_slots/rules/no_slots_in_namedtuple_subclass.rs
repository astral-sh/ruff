use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
#[derive(ViolationMetadata)]
pub(crate) struct NoSlotsInNamedtupleSubclass(NamedTupleKind);

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
    checker: &Checker,
    stmt: &Stmt,
    class: &StmtClassDef,
) {
    // https://github.com/astral-sh/ruff/issues/14535
    if checker.source_type.is_stub() {
        return;
    }
    let Some(Arguments { args: bases, .. }) = class.arguments.as_deref() else {
        return;
    };

    if let Some(namedtuple_kind) = namedtuple_base(bases, checker.semantic()) {
        if !has_slots(&class.body) {
            checker.report_diagnostic(Diagnostic::new(
                NoSlotsInNamedtupleSubclass(namedtuple_kind),
                stmt.identifier(),
            ));
        }
    }
}

/// If the class's bases consist solely of named tuples, return the kind of named tuple
/// (either `collections.namedtuple()`, or `typing.NamedTuple()`). Otherwise, return `None`.
fn namedtuple_base(bases: &[Expr], semantic: &SemanticModel) -> Option<NamedTupleKind> {
    let mut kind = None;
    for base in bases {
        if let Expr::Call(ast::ExprCall { func, .. }) = base {
            // Ex) `collections.namedtuple()`
            let qualified_name = semantic.resolve_qualified_name(func)?;
            match qualified_name.segments() {
                ["collections", "namedtuple"] => kind = kind.or(Some(NamedTupleKind::Collections)),
                ["typing", "NamedTuple"] => kind = kind.or(Some(NamedTupleKind::Typing)),
                // Ex) `enum.Enum`
                _ => return None,
            }
        } else if !semantic.match_builtin_expr(base, "object") {
            // Allow inheriting from `object`.

            return None;
        }
    }
    kind
}
