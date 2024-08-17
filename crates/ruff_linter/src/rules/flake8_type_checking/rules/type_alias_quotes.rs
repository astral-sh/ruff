use ast::ExprContext;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::TextRange;
use std::borrow::Borrow;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks if [PEP 613] explicit type aliases contain references to
/// symbols that are not available at runtime.
///
/// ## Why is this bad?
/// We will get a `NameError` at runtime.
///
/// ## Example
/// ```python
/// from typing import TYPE_CHECKING, TypeAlias
///
/// if TYPE_CHECKING:
///     from foo import Foo
/// OptFoo: TypeAlias = Foo | None
/// ```
///
/// Use instead:
/// ```python
/// from typing import TYPE_CHECKING, TypeAlias
///
/// if TYPE_CHECKING:
///     from foo import Foo
/// OptFoo: TypeAlias = "Foo | None"
/// ```
///
/// ## References
/// - [PEP 613](https://peps.python.org/pep-0613/)
///
/// [PEP 613]: https://peps.python.org/pep-0613/
#[violation]
pub struct UnquotedTypeAlias;

impl AlwaysFixableViolation for UnquotedTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Add quotes to type alias")
    }

    fn fix_title(&self) -> String {
        "Add quotes".to_string()
    }
}

/// ## What it does
/// Checks for unnecessary quotes in [PEP 613] explicit type aliases
/// and [PEP 695] type statements.
///
/// ## Why is this bad?
/// Unnecessary string forward references can lead to additional overhead
/// in runtime libraries making use of type hints, as well as lead to bad
/// interactions with other runtime uses like [PEP 604] type unions.
///
/// For explicit type aliases the quotes are only considered redundant
/// if the type expression contains no subscripts or attribute accesses
/// this is because of stubs packages. Some types will only be subscriptable
/// at type checking time, similarly there may be some module-level
/// attributes like type aliases that are only available in the stubs.
///
/// ## Example
/// Given:
/// ```python
/// OptInt: TypeAlias = "int | None"
/// ```
///
/// Use instead:
/// ```python
/// OptInt: TypeAlias = int | None
/// ```
///
/// Given:
/// ```python
/// type OptInt = "int | None"
/// ```
///
/// Use instead:
/// ```python
/// type OptInt = int | None
/// ```
///
/// ## References
/// - [PEP 613](https://peps.python.org/pep-0613/)
/// - [PEP 695](https://peps.python.org/pep-0695/#generic-type-alias)
/// - [PEP 604](https://peps.python.org/pep-0604/)
///
/// [PEP 604]: https://peps.python.org/pep-0604/
/// [PEP 613]: https://peps.python.org/pep-0613/
/// [PEP 695]: https://peps.python.org/pep-0695/#generic-type-alias
#[violation]
pub struct QuotedTypeAlias;

impl AlwaysFixableViolation for QuotedTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove quotes from type alias")
    }

    fn fix_title(&self) -> String {
        "Remove quotes".to_string()
    }
}

/// TCH007
/*pub(crate) fn unquoted_type_alias(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().in_explicit_type_alias() {
        return;
    }

    if checker.semantic().in_forward_reference() {
        return;
    }

    // TODO implement this
}*/

/// Traverses the type expression and checks the given predicate for each [`Binding`]
// TODO: Do we want to remove Attribute and Subscript traversal? We already
//       skip expressions that don't contain either. But then we can't reuse
//       this function for TCH007. Is it worth having two functions where one
//       has fewer branches because we know they won't be there?
fn contains_typing_reference(semantic: &SemanticModel, expr: &Expr) -> bool {
    match expr {
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            contains_typing_reference(semantic, left) || contains_typing_reference(semantic, right)
        }
        Expr::Starred(ast::ExprStarred {
            value,
            ctx: ExprContext::Load,
            ..
        })
        | Expr::Attribute(ast::ExprAttribute { value, .. }) => {
            contains_typing_reference(semantic, value)
        }
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            if contains_typing_reference(semantic, value) {
                return true;
            }
            if let Expr::Name(ast::ExprName { id, .. }) = value.borrow() {
                if id.as_str() != "Literal" {
                    return contains_typing_reference(semantic, slice);
                }
            }
            false
        }
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            for elt in elts {
                if contains_typing_reference(semantic, elt) {
                    return true;
                }
            }
            false
        }
        Expr::Name(name) => {
            semantic.lookup_symbol(name.id.as_str()).is_some()
                && semantic.simulate_runtime_load(name).is_none()
        }
        _ => false,
    }
}

/// TCH008
pub(crate) fn quoted_type_alias(
    checker: &mut Checker,
    expr: &Expr,
    annotation: &str,
    range: TextRange,
) {
    // explicit type aliases require some additional checks to avoid false positives
    if checker.semantic().in_explicit_type_alias() {
        // if the expression contains a subscript or attribute access
        if annotation.find(|c: char| c == '[' || c == '.').is_some() {
            return;
        }

        // if the expression contains references to typing-only bindings
        // then the quotes are not redundant
        if contains_typing_reference(checker.semantic(), expr) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(QuotedTypeAlias, range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        annotation.to_string(),
        range,
    )));
    checker.diagnostics.push(diagnostic);
}
