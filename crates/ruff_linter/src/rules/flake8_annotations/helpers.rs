use itertools::Itertools;
use ruff_python_ast::{BuildId, Builder, ListId};
use rustc_hash::FxHashSet;

use ruff_python_ast::StmtFunctionDef;
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_semantic::analyze::terminal::Terminal;
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_text_size::TextSize;

use crate::Edit;
use crate::checkers::ast::Checker;
use crate::fix::syntax;
use ruff_python_ast::PythonVersion;

/// Return the name of the function, if it's overloaded.
pub(crate) fn overloaded_name<'a>(
    definition: &'a Definition,
    semantic: &SemanticModel,
) -> Option<&'a str> {
    let function = definition.as_function_def()?;
    if visibility::is_overload(function.decorator_list(), semantic) {
        Some(function.name().as_str())
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
    if visibility::is_overload(function.decorator_list(), semantic) {
        false
    } else {
        function.name().as_str() == overloaded_name
    }
}

/// Given a function, guess its return type.
pub(crate) fn auto_return_type(
    function: StmtFunctionDef<'_>,
    semantic: &SemanticModel,
) -> Option<AutoPythonType> {
    // Collect all the `return` statements.
    let returns = {
        let visitor = ReturnStatementVisitor::for_function(function);

        // Ignore generators.
        if visitor.is_generator {
            return None;
        }

        visitor.returns
    };

    // Determine the terminal behavior (i.e., implicit return, no return, etc.).
    let terminal = Terminal::from_function(function, semantic);

    // If every control flow path raises NotImplementedError, don't suggest NoReturn
    // since these are abstract methods that should have the actual return type.
    if terminal == Terminal::RaiseNotImplemented {
        return None;
    }

    // If every control flow path raises an exception (other than NotImplementedError),
    // suggest NoReturn.
    if terminal == Terminal::Raise {
        return Some(AutoPythonType::Never);
    }

    // Determine the return type of the first `return` statement.
    let Some((return_statement, returns)) = returns.split_first() else {
        return Some(AutoPythonType::Atom(PythonType::None));
    };
    let mut return_type = return_statement.value().map_or(
        ResolvedPythonType::Atom(PythonType::None),
        ResolvedPythonType::from,
    );

    // Merge the return types of the remaining `return` statements.
    for return_statement in returns {
        return_type = return_type.union(return_statement.value().map_or(
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
    /// Renders the return annotation and collects the import edits it needs at `at`.
    pub(crate) fn into_annotation(
        self,
        checker: &Checker,
        at: TextSize,
    ) -> Option<(String, Vec<Edit>)> {
        let target_version = checker.target_version();
        let mut builder = Builder::default();
        let (expression, edits) = match self {
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
                let expr = syntax::name(&mut builder, &binding);
                Some((expr, vec![no_return_edit]))
            }
            AutoPythonType::Atom(python_type) => type_expr(&mut builder, python_type, checker, at),
            AutoPythonType::Union(python_types) => {
                if target_version >= PythonVersion::PY310 {
                    // Aggregate all the individual types (e.g., `int`, `float`).
                    let mut all_edits = Vec::new();
                    let names = python_types
                        .iter()
                        .sorted_unstable()
                        .map(|python_type| {
                            let (expr, mut edits) =
                                type_expr(&mut builder, *python_type, checker, at)?;
                            all_edits.append(&mut edits);
                            Some(expr)
                        })
                        .collect::<Option<Vec<_>>>()?;

                    // Wrap in a bitwise union (e.g., `int | float`).
                    let expr = names
                        .into_iter()
                        .reduce(|left, right| syntax::bit_or(&mut builder, left, right))
                        .unwrap_or_else(|| syntax::tuple(&mut builder, ListId::default()));

                    Some((expr, all_edits))
                } else {
                    let python_types = python_types
                        .into_iter()
                        .sorted_unstable()
                        .collect::<Vec<_>>();

                    match python_types.as_slice() {
                        [python_type, PythonType::None] | [PythonType::None, python_type] => {
                            let (element, mut edits) =
                                type_expr(&mut builder, *python_type, checker, at)?;

                            // Ex) `Optional[int]`
                            let (optional_edit, binding) = checker
                                .typing_importer("Optional", PythonVersion::lowest())?
                                .import(at)
                                .ok()?;
                            let optional = syntax::name(&mut builder, &binding);
                            let expr = syntax::subscript(&mut builder, optional, element);
                            edits.push(optional_edit);
                            Some((expr, edits))
                        }
                        _ => {
                            let mut all_edits = Vec::new();
                            let elements = python_types
                                .into_iter()
                                .map(|python_type| {
                                    let (expr, mut edits) =
                                        type_expr(&mut builder, python_type, checker, at)?;
                                    all_edits.append(&mut edits);
                                    Some(expr)
                                })
                                .collect::<Option<Vec<_>>>()?;

                            // Ex) `Union[int, str]`
                            let (union_edit, binding) = checker
                                .typing_importer("Union", PythonVersion::lowest())?
                                .import(at)
                                .ok()?;
                            let union = syntax::name(&mut builder, &binding);
                            let elts = builder.list(elements);
                            let expr = syntax::subscript_tuple(&mut builder, union, elts);
                            all_edits.push(union_edit);
                            Some((expr, all_edits))
                        }
                    }
                }
            }
        }?;
        Some((checker.generator().expr(builder.get(expression)), edits))
    }
}

fn type_expr(
    builder: &mut Builder,
    python_type: PythonType,
    checker: &Checker,
    at: TextSize,
) -> Option<(BuildId<ruff_python_ast::kind::Expr>, Vec<Edit>)> {
    let symbol = match python_type {
        PythonType::String => "str",
        PythonType::Bytes => "bytes",
        PythonType::Number(number) => match number {
            NumberLike::Integer => "int",
            NumberLike::Float => "float",
            NumberLike::Complex => "complex",
            NumberLike::Bool => "bool",
        },
        PythonType::None => return Some((syntax::none(builder), Vec::new())),
        PythonType::Ellipsis
        | PythonType::Dict
        | PythonType::List
        | PythonType::Set
        | PythonType::Tuple
        | PythonType::Generator => return None,
    };
    let (edit, binding) = checker
        .importer()
        .get_or_import_builtin_symbol(symbol, at, checker.semantic())
        .ok()?;
    Some((syntax::name(builder, &binding), edit.into_iter().collect()))
}
