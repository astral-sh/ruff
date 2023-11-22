use itertools::Itertools;

use ruff_python_ast::helpers::{pep_604_union, ReturnStatementVisitor};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, ExprContext};
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_text_size::TextRange;

use crate::settings::types::PythonVersion;

/// Return the name of the function, if it's overloaded.
pub(crate) fn overloaded_name(definition: &Definition, semantic: &SemanticModel) -> Option<String> {
    let function = definition.as_function_def()?;
    if visibility::is_overload(&function.decorator_list, semantic) {
        Some(function.name.to_string())
    } else {
        None
    }
}

/// Return `true` if the definition is the implementation for an overloaded
/// function.
pub(crate) fn is_overload_impl(
    definition: &Definition,
    overloaded_name: &str,
    semantic: &SemanticModel,
) -> bool {
    let Some(function) = definition.as_function_def() else {
        return false;
    };
    if visibility::is_overload(&function.decorator_list, semantic) {
        false
    } else {
        function.name.as_str() == overloaded_name
    }
}

/// Given a function, guess its return type.
pub(crate) fn auto_return_type(
    function: &ast::StmtFunctionDef,
    target_version: PythonVersion,
) -> Option<Expr> {
    // Collect all the `return` statements.
    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(&function.body);
        if visitor.is_generator {
            return None;
        }
        visitor.returns
    };

    // Determine the return type of the first `return` statement.
    let (return_statement, returns) = returns.split_first()?;
    let mut return_type = return_statement.value.as_deref().map_or(
        ResolvedPythonType::Atom(PythonType::None),
        ResolvedPythonType::from,
    );

    // Merge the return types of the remaining `return` statements.
    for return_statement in returns {
        return_type = return_type.union(return_statement.value.as_deref().map_or(
            ResolvedPythonType::Atom(PythonType::None),
            ResolvedPythonType::from,
        ));
    }

    match return_type {
        ResolvedPythonType::Atom(python_type) => type_expr(python_type),
        ResolvedPythonType::Union(python_types) if target_version >= PythonVersion::Py310 => {
            // Aggregate all the individual types (e.g., `int`, `float`).
            let names = python_types
                .iter()
                .sorted_unstable()
                .filter_map(|python_type| type_expr(*python_type))
                .collect::<Vec<_>>();

            // Wrap in a bitwise union (e.g., `int | float`).
            Some(pep_604_union(&names))
        }
        ResolvedPythonType::Union(_) => None,
        ResolvedPythonType::Unknown => None,
        ResolvedPythonType::TypeError => None,
    }
}

/// Given a [`PythonType`], return an [`Expr`] that resolves to that type.
fn type_expr(python_type: PythonType) -> Option<Expr> {
    fn name(name: &str) -> Expr {
        Expr::Name(ast::ExprName {
            id: name.into(),
            range: TextRange::default(),
            ctx: ExprContext::Load,
        })
    }

    match python_type {
        PythonType::String => Some(name("str")),
        PythonType::Bytes => Some(name("bytes")),
        PythonType::Number(number) => match number {
            NumberLike::Integer => Some(name("int")),
            NumberLike::Float => Some(name("float")),
            NumberLike::Complex => Some(name("complex")),
            NumberLike::Bool => Some(name("bool")),
        },
        PythonType::None => Some(name("None")),
        PythonType::Ellipsis => None,
        PythonType::Dict => None,
        PythonType::List => None,
        PythonType::Set => None,
        PythonType::Tuple => None,
        PythonType::Generator => None,
    }
}
