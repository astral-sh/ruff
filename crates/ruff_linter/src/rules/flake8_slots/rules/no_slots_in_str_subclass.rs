use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{Arguments, Expr, Stmt, StmtClassDef};
use ruff_python_semantic::{analyze::class::is_enumeration, SemanticModel};

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
#[derive(ViolationMetadata)]
pub(crate) struct NoSlotsInStrSubclass;

impl Violation for NoSlotsInStrSubclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Subclasses of `str` should define `__slots__`".to_string()
    }
}

/// SLOT000
pub(crate) fn no_slots_in_str_subclass(checker: &Checker, stmt: &Stmt, class: &StmtClassDef) {
    // https://github.com/astral-sh/ruff/issues/14535
    if checker.source_type.is_stub() {
        return;
    }
    let Some(Arguments { args: bases, .. }) = class.arguments.as_deref() else {
        return;
    };

    let semantic = checker.semantic();

    if !is_str_subclass(bases, semantic) {
        return;
    }

    // Ignore subclasses of `enum.Enum` et al.
    if is_enumeration(class, semantic) {
        return;
    }

    if has_slots(&class.body) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(NoSlotsInStrSubclass, stmt.identifier()));
}

/// Return `true` if the class is a subclass of `str`.
fn is_str_subclass(bases: &[Expr], semantic: &SemanticModel) -> bool {
    bases
        .iter()
        .any(|base| semantic.match_builtin_expr(base, "str"))
}
