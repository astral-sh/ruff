use ruff_python_ast::Expr;
use ruff_python_semantic::{
    BindingKind, Imported, SemanticModel, analyze::typing::find_binding_value,
};

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
pub(super) fn test_expression(expr: &Expr, semantic: &SemanticModel) -> Resolution {
    match expr {
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::Tuple(_)
        | Expr::List(_)
        | Expr::Set(_)
        | Expr::Dict(_)
        | Expr::SetComp(_)
        | Expr::ListComp(_)
        | Expr::DictComp(_)
        | Expr::Generator(_) => Resolution::IrrelevantExpression,
        Expr::Name(name) => {
            semantic
                .resolve_name(name)
                .map_or(Resolution::IrrelevantBinding, |id| {
                    let binding = semantic.binding(id);
                    match &binding.kind {
                        BindingKind::Argument => {
                            // Avoid, e.g., `self.values`.
                            if matches!(name.id.as_str(), "self" | "cls") {
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
                        | BindingKind::Nonlocal(_, _) => {
                            // Check if this binding comes from pandas or another relevant source
                            if let Some(assigned_value) = find_binding_value(binding, semantic) {
                                // Check if the assigned value comes from pandas
                                if is_pandas_related_value(assigned_value, semantic) {
                                    Resolution::RelevantLocal
                                } else {
                                    // This is a non-pandas binding (e.g., literal, numpy, etc.)
                                    Resolution::IrrelevantBinding
                                }
                            } else {
                                // If we can't determine the source, be conservative and treat as relevant
                                Resolution::RelevantLocal
                            }
                        }
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

/// Check if an expression value is related to pandas (e.g., comes from pandas module or operations).
fn is_pandas_related_value(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        // Literals are definitely not pandas-related
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::Tuple(_)
        | Expr::List(_)
        | Expr::Set(_)
        | Expr::Dict(_) => false,

        // Direct pandas module access
        Expr::Name(name) => {
            if let Some(binding_id) = semantic.resolve_name(name) {
                let binding = semantic.binding(binding_id);
                if let BindingKind::Import(import) = &binding.kind {
                    return matches!(import.qualified_name().segments(), ["pandas"]);
                }
            }
            false
        }
        // Method calls on pandas objects
        Expr::Attribute(attr) => {
            // Check if the object being accessed is pandas-related
            is_pandas_related_value(attr.value.as_ref(), semantic)
        }
        // Function calls - check if they're pandas functions
        Expr::Call(call) => {
            if let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) {
                return qualified_name.segments().starts_with(&["pandas"]);
            }
            false
        }
        // For other expressions, we can't easily determine if they're pandas-related
        // so we return false to be conservative (treat as non-pandas)
        _ => false,
    }
}
