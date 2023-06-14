use ruff_python_ast::helpers::map_callable;
use rustpython_parser::ast::{self, Expr};

use ruff_python_semantic::SemanticModel;

pub(super) fn is_mutable_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::List(_)
            | Expr::Dict(_)
            | Expr::Set(_)
            | Expr::ListComp(_)
            | Expr::DictComp(_)
            | Expr::SetComp(_)
    )
}

const ALLOWED_DATACLASS_SPECIFIC_FUNCTIONS: &[&[&str]] = &[&["dataclasses", "field"]];

pub(super) fn is_allowed_dataclass_function(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(func).map_or(false, |call_path| {
        ALLOWED_DATACLASS_SPECIFIC_FUNCTIONS
            .iter()
            .any(|target| call_path.as_slice() == *target)
    })
}

/// Returns `true` if the given [`Expr`] is a `typing.ClassVar` annotation.
pub(super) fn is_class_var_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Subscript(ast::ExprSubscript { value, .. }) = &annotation else {
        return false;
    };
    semantic.match_typing_expr(value, "ClassVar")
}

/// Returns `true` if the given class is a dataclass.
pub(super) fn is_dataclass(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    class_def.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["dataclasses", "dataclass"])
            })
    })
}
