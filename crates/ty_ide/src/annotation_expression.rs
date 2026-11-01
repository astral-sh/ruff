//! Identifies ranges inside annotation expressions.

use ruff_python_ast::find_node::CoveringNode;
use ruff_python_ast::{self as ast, AnyKind};
use ruff_text_size::{Ranged, TextRange};
use ty_python_semantic::SemanticModel;

/// Returns whether `range` is syntactically enclosed by a parameter, return, or assignment
/// annotation; an explicit type alias value; or a type parameter's bound, constraints, or default.
///
/// This is an approximation of the typing specification's [annotation expressions]. It currently
/// has several known false positives and false negatives. For example, it can return `true` for value
/// expressions in `Annotated` metadata and `Literal` arguments, and `false` for type expressions in
/// the first argument to `cast()`, the constraints of `TypeVar()`, and field types in functional
/// `NamedTuple` and `TypedDict` declarations.
///
/// `covering_node` must cover `range` in the file represented by `model`.
///
/// [annotation expressions]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
pub(crate) fn is_in_annotation_expression(
    model: &SemanticModel<'_>,
    covering_node: &CoveringNode<'_>,
    range: TextRange,
) -> bool {
    let contains = |expr: ast::Expr<'_>| expr.range().contains_range(range);

    covering_node.ancestors().any(|node| match node.classify() {
        AnyKind::StmtAnnAssign(stmt) => {
            contains(stmt.annotation())
                || (stmt.value().is_some_and(contains)
                    && model.is_type_alias_annotation(stmt.annotation()))
        }
        AnyKind::StmtFunctionDef(stmt) => stmt.returns().is_some_and(contains),
        AnyKind::StmtTypeAlias(stmt) => contains(stmt.value()),
        AnyKind::Parameter(param) => param.annotation().is_some_and(contains),
        AnyKind::TypeParamTypeVar(type_param) => {
            type_param.bound().is_some_and(contains) || type_param.default().is_some_and(contains)
        }
        AnyKind::TypeParamTypeVarTuple(type_param) => type_param.default().is_some_and(contains),
        AnyKind::TypeParamParamSpec(type_param) => type_param.default().is_some_and(contains),
        _ => false,
    })
}
