use rustpython_parser::ast;
use rustpython_parser::ast::Expr;

use ruff_python_semantic::binding::{BindingKind, Importation};
use ruff_python_semantic::model::SemanticModel;

pub(crate) enum Resolution {
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
pub(crate) fn test_expression(expr: &Expr, model: &SemanticModel) -> Resolution {
    match expr {
        Expr::Constant(_)
        | Expr::Tuple(_)
        | Expr::List(_)
        | Expr::Set(_)
        | Expr::Dict(_)
        | Expr::SetComp(_)
        | Expr::ListComp(_)
        | Expr::DictComp(_)
        | Expr::GeneratorExp(_) => Resolution::IrrelevantExpression,
        Expr::Name(ast::ExprName { id, .. }) => {
            model
                .find_binding(id)
                .map_or(Resolution::IrrelevantBinding, |binding| {
                    match binding.kind {
                        BindingKind::Annotation
                        | BindingKind::Argument
                        | BindingKind::Assignment
                        | BindingKind::NamedExprAssignment
                        | BindingKind::Binding
                        | BindingKind::LoopVar
                        | BindingKind::Global
                        | BindingKind::Nonlocal => Resolution::RelevantLocal,
                        BindingKind::Importation(Importation {
                            full_name: module, ..
                        }) if module == "pandas" => Resolution::PandasModule,
                        _ => Resolution::IrrelevantBinding,
                    }
                })
        }
        _ => Resolution::RelevantLocal,
    }
}
