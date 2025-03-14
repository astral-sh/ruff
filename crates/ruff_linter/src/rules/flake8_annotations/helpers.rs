use itertools::Itertools;
use rustc_hash::FxHashSet;

use ruff_diagnostics::Edit;
use ruff_python_ast::helpers::{
    pep_604_union, typing_optional, typing_union, ReturnStatementVisitor,
};
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, ExprContext};
use ruff_python_semantic::analyze::terminal::Terminal;
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_text_size::{TextRange, TextSize};

use crate::importer::{ImportRequest, Importer};
use ruff_python_ast::PythonVersion;

/// Return the name of the function, if it's overloaded.
pub(crate) fn overloaded_name<'a>(
    definition: &'a Definition,
    semantic: &SemanticModel,
) -> Option<&'a str> {
    let function = definition.as_function_def()?;
    if visibility::is_overload(&function.decorator_list, semantic) {
        Some(function.name.as_str())
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
pub(crate) fn auto_return_type(function: &ast::StmtFunctionDef) -> Option<AutoPythonType> {
    // Collect all the `return` statements.
    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(&function.body);

        // Ignore generators.
        if visitor.is_generator {
            return None;
        }

        visitor.returns
    };

    // Determine the terminal behavior (i.e., implicit return, no return, etc.).
    let terminal = Terminal::from_function(function);

    // If every control flow path raises an exception, return `NoReturn`.
    if terminal == Terminal::Raise {
        return Some(AutoPythonType::Never);
    }

    // Determine the return type of the first `return` statement.
    let Some((return_statement, returns)) = returns.split_first() else {
        return Some(AutoPythonType::Atom(PythonType::None));
    };
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

    // If the function has an implicit return, union with `None`, as in:
    // ```python
    // def func(x: int):
    //     if x > 0:
    //         return 1
    // ```
    if terminal.has_implicit_return() {
        return_type = return_type.union(ResolvedPythonType::Atom(PythonType::None));
    }

    match return_type {
        ResolvedPythonType::Atom(python_type) => Some(AutoPythonType::Atom(python_type)),
        ResolvedPythonType::Union(python_types) => Some(AutoPythonType::Union(python_types)),
        ResolvedPythonType::Unknown => None,
        ResolvedPythonType::TypeError => None,
    }
}

#[derive(Debug)]
pub(crate) enum AutoPythonType {
    Never,
    Atom(PythonType),
    Union(FxHashSet<PythonType>),
}

impl AutoPythonType {
    /// Convert an [`AutoPythonType`] into an [`Expr`].
    ///
    /// If the [`Expr`] relies on importing any external symbols, those imports will be returned as
    /// additional edits.
    pub(crate) fn into_expression(
        self,
        importer: &Importer,
        at: TextSize,
        semantic: &SemanticModel,
        target_version: PythonVersion,
    ) -> Option<(Expr, Vec<Edit>)> {
        match self {
            AutoPythonType::Never => {
                let (no_return_edit, binding) = importer
                    .get_or_import_symbol(
                        &ImportRequest::import_from(
                            "typing",
                            if target_version >= PythonVersion::PY311 {
                                "Never"
                            } else {
                                "NoReturn"
                            },
                        ),
                        at,
                        semantic,
                    )
                    .ok()?;
                let expr = Expr::Name(ast::ExprName {
                    id: Name::from(binding),
                    range: TextRange::default(),
                    ctx: ExprContext::Load,
                });
                Some((expr, vec![no_return_edit]))
            }
            AutoPythonType::Atom(python_type) => {
                let expr = type_expr(python_type)?;
                Some((expr, vec![]))
            }
            AutoPythonType::Union(python_types) => {
                if target_version >= PythonVersion::PY310 {
                    // Aggregate all the individual types (e.g., `int`, `float`).
                    let names = python_types
                        .iter()
                        .sorted_unstable()
                        .map(|python_type| type_expr(*python_type))
                        .collect::<Option<Vec<_>>>()?;

                    // Wrap in a bitwise union (e.g., `int | float`).
                    let expr = pep_604_union(&names);

                    Some((expr, vec![]))
                } else {
                    let python_types = python_types
                        .into_iter()
                        .sorted_unstable()
                        .collect::<Vec<_>>();

                    match python_types.as_slice() {
                        [python_type, PythonType::None] | [PythonType::None, python_type] => {
                            let element = type_expr(*python_type)?;

                            // Ex) `Optional[int]`
                            let (optional_edit, binding) = importer
                                .get_or_import_symbol(
                                    &ImportRequest::import_from("typing", "Optional"),
                                    at,
                                    semantic,
                                )
                                .ok()?;
                            let expr = typing_optional(element, Name::from(binding));
                            Some((expr, vec![optional_edit]))
                        }
                        _ => {
                            let elements = python_types
                                .into_iter()
                                .map(type_expr)
                                .collect::<Option<Vec<_>>>()?;

                            // Ex) `Union[int, str]`
                            let (union_edit, binding) = importer
                                .get_or_import_symbol(
                                    &ImportRequest::import_from("typing", "Union"),
                                    at,
                                    semantic,
                                )
                                .ok()?;
                            let expr = typing_union(&elements, Name::from(binding));
                            Some((expr, vec![union_edit]))
                        }
                    }
                }
            }
        }
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
