use ruff_python_ast::Expr;
use ruff_python_ast::flat;
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
pub(super) fn test_expression(expr: flat::View<'_, Expr>, semantic: &SemanticModel) -> Resolution {
    match expr.classify() {
        flat::ExprKind::StringLiteral(_)
        | flat::ExprKind::BytesLiteral(_)
        | flat::ExprKind::NumberLiteral(_)
        | flat::ExprKind::BooleanLiteral(_)
        | flat::ExprKind::NoneLiteral(_)
        | flat::ExprKind::EllipsisLiteral(_)
        | flat::ExprKind::Tuple(_)
        | flat::ExprKind::List(_)
        | flat::ExprKind::Set(_)
        | flat::ExprKind::Dict(_)
        | flat::ExprKind::SetComp(_)
        | flat::ExprKind::ListComp(_)
        | flat::ExprKind::DictComp(_)
        | flat::ExprKind::Generator(_) => Resolution::IrrelevantExpression,
        flat::ExprKind::Name(name) => {
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
