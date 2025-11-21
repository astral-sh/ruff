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
        // Literals in the expression itself are definitely not pandas-related
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
                                // Function arguments are treated as relevant unless proven otherwise
                                Resolution::RelevantLocal
                            }
                        }
                        BindingKind::Annotation
                        | BindingKind::Assignment
                        | BindingKind::NamedExprAssignment
                        | BindingKind::LoopVar
                        | BindingKind::Global(_)
                        | BindingKind::Nonlocal(_, _) => {
                            // Check if this binding comes from a definitively non-pandas source
                            if let Some(assigned_value) = find_binding_value(binding, semantic) {
                                // Recurse to check the assigned value
                                match test_expression(assigned_value, semantic) {
                                    // If the assigned value is definitively not pandas (literals, etc.)
                                    Resolution::IrrelevantExpression => {
                                        Resolution::IrrelevantBinding
                                    }
                                    // If it's clearly pandas-related, treat as relevant
                                    Resolution::RelevantLocal | Resolution::PandasModule => {
                                        Resolution::RelevantLocal
                                    }
                                    // If we got IrrelevantBinding, it means we traced it back to a
                                    // non-pandas source (e.g., numpy import), so keep it as irrelevant
                                    Resolution::IrrelevantBinding => Resolution::IrrelevantBinding,
                                }
                            } else {
                                // If we can't determine the source, be liberal and treat as relevant
                                // to avoid false negatives (e.g., function parameters with annotations)
                                Resolution::RelevantLocal
                            }
                        }
                        BindingKind::Import(import) => {
                            let segments = import.qualified_name().segments();
                            if matches!(segments, ["pandas"]) {
                                Resolution::PandasModule
                            } else if matches!(segments, ["numpy"]) {
                                // Explicitly exclude numpy imports
                                Resolution::IrrelevantBinding
                            } else {
                                Resolution::IrrelevantBinding
                            }
                        }
                        _ => Resolution::IrrelevantBinding,
                    }
                })
        }
        // Recurse for attribute access (e.g., df.values -> check df)
        Expr::Attribute(attr) => test_expression(attr.value.as_ref(), semantic),
        // Recurse for call expressions (e.g., pd.DataFrame() -> check pd)
        Expr::Call(call) => {
            // Check if this is a pandas function call
            if let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) {
                let segments = qualified_name.segments();
                if segments.starts_with(&["pandas"]) {
                    return Resolution::RelevantLocal;
                }
                // Explicitly exclude numpy function calls
                if segments.starts_with(&["numpy"]) || segments.starts_with(&["np"]) {
                    return Resolution::IrrelevantBinding;
                }
            }
            // For other calls, recurse on the function expression
            test_expression(&call.func, semantic)
        }
        // For other expressions, default to relevant to avoid false negatives
        _ => Resolution::RelevantLocal,
    }
}
