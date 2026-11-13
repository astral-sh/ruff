use ruff_python_ast::{Expr, ExprKind};
use ruff_python_semantic::{BindingKind, Imported, SemanticModel};

#[derive(Debug)]
pub(super) enum Resolution {
    /// The expression resolves to an irrelevant expression type (e.g., a constant).
    IrrelevantExpression,
    /// The expression resolves to an irrelevant binding (e.g., a function definition).
    IrrelevantBinding,
    /// The expression resolves to a relevant local binding (e.g., a variable).
    RelevantLocal,
    /// The expression resolves to the `pandas` module itself.
    PandasModule,
}

/// Test an [`Expr`] for relevance to Pandas-related operations.
pub(super) fn test_expression(expr: Expr<'_>, semantic: &SemanticModel) -> Resolution {
    match expr.classify() {
        ExprKind::StringLiteral(_)
        | ExprKind::BytesLiteral(_)
        | ExprKind::NumberLiteral(_)
        | ExprKind::BooleanLiteral(_)
        | ExprKind::NoneLiteral(_)
        | ExprKind::EllipsisLiteral(_)
        | ExprKind::Tuple(_)
        | ExprKind::List(_)
        | ExprKind::Set(_)
        | ExprKind::Dict(_)
        | ExprKind::SetComp(_)
        | ExprKind::ListComp(_)
        | ExprKind::DictComp(_)
        | ExprKind::Generator(_) => Resolution::IrrelevantExpression,
        ExprKind::Name(name) => {
            semantic
                .resolve_name(name)
                .map_or(Resolution::IrrelevantBinding, |id| {
                    match &semantic.binding(id).kind {
                        BindingKind::Argument => {
                            // Avoid, e.g., `self.values`.
                            if matches!(name.id().as_str(), "self" | "cls") {
                                Resolution::IrrelevantBinding
                            } else {
                                Resolution::RelevantLocal
                            }
                        }
                        BindingKind::Annotation
                        | BindingKind::Assignment
                        | BindingKind::NamedExprAssignment
                        | BindingKind::LoopVar
                        | BindingKind::Global(_)
                        | BindingKind::Nonlocal(_, _) => Resolution::RelevantLocal,
                        BindingKind::Import(import)
                            if matches!(import.qualified_name().segments(), ["pandas"]) =>
                        {
                            Resolution::PandasModule
                        }
                        _ => Resolution::IrrelevantBinding,
                    }
                })
        }
        _ => Resolution::RelevantLocal,
    }
}
