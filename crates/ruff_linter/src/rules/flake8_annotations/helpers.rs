use itertools::Itertools;
use rustc_hash::FxHashSet;

use ruff_python_ast::helpers::{
    ReturnStatementVisitor, pep_604_union, typing_optional, typing_union,
};
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, ExprContext};
use ruff_python_semantic::analyze::terminal::Terminal;
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_text_size::{TextRange, TextSize};

use crate::Edit;
use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;
use ruff_python_ast::visitor::walk_stmt;
use ruff_python_ast::{ExprCall, ExprName, Stmt};

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

    if terminal == Terminal::Raise {
        if raises_only_not_implemented_error(function) {
            return None;
        }

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
        checker: &Checker,
        at: TextSize,
    ) -> Option<(Expr, Vec<Edit>)> {
        let target_version = checker.target_version();
        match self {
            AutoPythonType::Never => {
                let member = if target_version >= PythonVersion::PY311 {
                    "Never"
                } else {
                    "NoReturn"
                };
                let (no_return_edit, binding) = checker
                    .typing_importer(member, PythonVersion::lowest())?
                    .import(at)
                    .ok()?;
                let expr = Expr::Name(ast::ExprName {
                    id: Name::from(binding),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                    ctx: ExprContext::Load,
                });
                Some((expr, vec![no_return_edit]))
            }
            AutoPythonType::Atom(python_type) => type_expr(python_type, checker, at),
            AutoPythonType::Union(python_types) => {
                if target_version >= PythonVersion::PY310 {
                    // Aggregate all the individual types (e.g., `int`, `float`).
                    let mut all_edits = Vec::new();
                    let names = python_types
                        .iter()
                        .sorted_unstable()
                        .map(|python_type| {
                            let (expr, mut edits) = type_expr(*python_type, checker, at)?;
                            all_edits.append(&mut edits);
                            Some(expr)
                        })
                        .collect::<Option<Vec<_>>>()?;

                    // Wrap in a bitwise union (e.g., `int | float`).
                    let expr = pep_604_union(&names);

                    Some((expr, all_edits))
                } else {
                    let python_types = python_types
                        .into_iter()
                        .sorted_unstable()
                        .collect::<Vec<_>>();

                    match python_types.as_slice() {
                        [python_type, PythonType::None] | [PythonType::None, python_type] => {
                            let (element, mut edits) = type_expr(*python_type, checker, at)?;

                            // Ex) `Optional[int]`
                            let (optional_edit, binding) = checker
                                .typing_importer("Optional", PythonVersion::lowest())?
                                .import(at)
                                .ok()?;
                            let expr = typing_optional(element, Name::from(binding));
                            edits.push(optional_edit);
                            Some((expr, edits))
                        }
                        _ => {
                            let mut all_edits = Vec::new();
                            let elements = python_types
                                .into_iter()
                                .map(|python_type| {
                                    let (expr, mut edits) = type_expr(python_type, checker, at)?;
                                    all_edits.append(&mut edits);
                                    Some(expr)
                                })
                                .collect::<Option<Vec<_>>>()?;

                            // Ex) `Union[int, str]`
                            let (union_edit, binding) = checker
                                .typing_importer("Union", PythonVersion::lowest())?
                                .import(at)
                                .ok()?;
                            let expr = typing_union(&elements, Name::from(binding));
                            all_edits.push(union_edit);
                            Some((expr, all_edits))
                        }
                    }
                }
            }
        }
    }
}

/// Given a [`PythonType`], return an [`Expr`] that resolves to that type.
///
/// If the [`Expr`] relies on importing any external symbols, those imports will be returned as
/// additional edits.
pub(crate) fn type_expr(
    python_type: PythonType,
    checker: &Checker,
    at: TextSize,
) -> Option<(Expr, Vec<Edit>)> {
    fn name(name: &str, checker: &Checker, at: TextSize) -> Option<(Expr, Vec<Edit>)> {
        let (edit, binding) = checker
            .importer()
            .get_or_import_builtin_symbol(name, at, checker.semantic())
            .ok()?;
        let expr = Expr::Name(ast::ExprName {
            id: binding.into(),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            ctx: ExprContext::Load,
        });
        Some((expr, edit.map_or_else(Vec::new, |edit| vec![edit])))
    }

    match python_type {
        PythonType::String => name("str", checker, at),
        PythonType::Bytes => name("bytes", checker, at),
        PythonType::Number(number) => {
            let symbol = match number {
                NumberLike::Integer => "int",
                NumberLike::Float => "float",
                NumberLike::Complex => "complex",
                NumberLike::Bool => "bool",
            };
            name(symbol, checker, at)
        }
        PythonType::None => {
            let expr = Expr::NoneLiteral(ast::ExprNoneLiteral::default());
            Some((expr, vec![]))
        }
        PythonType::Ellipsis => None,
        PythonType::Dict => None,
        PythonType::List => None,
        PythonType::Set => None,
        PythonType::Tuple => None,
        PythonType::Generator => None,
    }
}

fn raises_only_not_implemented_error(function: &ast::StmtFunctionDef) -> bool {
    struct RaiseVisitor {
        only_not_implemented: bool,
        seen_any_raise: bool,
    }

    impl<'a> Visitor<'a> for RaiseVisitor {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            if let Stmt::Raise(ast::StmtRaise { exc, .. }) = stmt {
                self.seen_any_raise = true;

                let Some(exc) = exc.as_deref() else {
                    self.only_not_implemented = false;
                    return;
                };

                let is_not_impl = match exc {
                    Expr::Name(ExprName { id, .. }) => id == "NotImplementedError",
                    Expr::Call(ExprCall { func, .. }) => match func.as_ref() {
                        Expr::Name(ExprName { id, .. }) => id == "NotImplementedError",
                        _ => false,
                    },
                    _ => false,
                };

                if !is_not_impl {
                    self.only_not_implemented = false;
                }
            } else {
                walk_stmt(self, stmt);
            }
        }
    }

    let mut visitor = RaiseVisitor {
        only_not_implemented: true,
        seen_any_raise: false,
    };

    visitor.visit_body(&function.body);

    visitor.seen_any_raise && visitor.only_not_implemented
}
