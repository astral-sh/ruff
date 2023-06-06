use ruff_text_size::TextRange;
use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, StmtClassDef};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::rules::flake8_slots::rules::helpers::has_slots;

/// ## What it does
/// Checks if subclasses of `collections.namedtuple` have not defined a value for `__slots__`
///
/// ## Why is this bad?
/// `__slots__` allow us to explicitly declare data members (like properties) and deny the creation
/// of `__dict__` and `__weakref__` (unless explicitly declared in `__slots__` or available in a
/// parent.) The space saved over using `__dict__` can be significant. Attribute lookup speed can be
/// significantly improved as well.
///
/// ## Example
/// ```python
/// from collections import namedtuple
///
/// class Foo(namedtuple("foo", ["name", "age"]):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// from collections import namedtuple
///
/// class Foo(namedtuple("foo", ["name", "age"]):
///     __slots__ = ()
/// ```
/// ## References
/// - [Python Docs](https://docs.python.org/3.7/reference/datamodel.html#slots)
/// - [StackOverflow](https://stackoverflow.com/questions/472000/usage-of-slots)
#[violation]
pub struct NoSlotsInNamedtupleSubclass;

impl Violation for NoSlotsInNamedtupleSubclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Subclasses of `collections.namedtuple()` should define `__slots__`")
    }
}

/// SLOT002
pub(crate) fn no_slots_in_namedtuple_subclass<F>(
    checker: &mut Checker,
    class: &StmtClassDef,
    locate: F,
) where
    F: FnOnce() -> TextRange,
{
    if class.bases.len() != 1 {
        return;
    }

    let Expr::Call(ast::ExprCall { func, .. }) = &class.bases[0] else {
        return;
    };

    if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
        if id.as_str() == "namedtuple"
            && checker
                .semantic_model()
                .resolve_call_path(func)
                .map_or(false, |call_path| {
                    matches!(call_path.as_slice(), ["collections", "namedtuple"])
                })
        {
            if !has_slots(&class.body) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(NoSlotsInNamedtupleSubclass, locate()));
            }
        }
    }
}
