use ast::{ParameterWithDefault, Parameters};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::{Binding, BindingKind, SemanticModel};

/// Abstraction for a type checker, conservatively checks for the intended type(s).
trait TypeChecker {
    /// Check annotation expression to match the intended type(s).
    fn match_annotation(semantic: &SemanticModel, annotation: &Expr) -> bool;
    /// Check initializer expression to match the intended type(s).
    fn match_initializer(semantic: &SemanticModel, initializer: &Expr) -> bool;
}

/// Check if the type checker accepts the given binding with the given name.
///
/// NOTE: this function doesn't perform more serious type inference, so it won't be able
///       to understand if the value gets initialized from a call to a function always returning
///       lists. This also implies no interfile analysis.
fn check_type<T: TypeChecker>(semantic: &SemanticModel, binding: &Binding, name: &str) -> bool {
    assert!(binding.source.is_some());
    let stmt = semantic.statement(binding.source.unwrap());

    match binding.kind {
        BindingKind::Assignment => match stmt {
            // ```python
            // x = init_expr
            // ```
            //
            // The type checker might know how to infer the type based on `init_expr`.
            Stmt::Assign(ast::StmtAssign { value, .. }) => {
                T::match_initializer(semantic, value.as_ref())
            }

            // ```python
            // x: annotation = some_expr
            // ```
            //
            // In this situation, we check only the annotation.
            Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. }) => {
                T::match_annotation(semantic, annotation.as_ref())
            }
            _ => false,
        },

        BindingKind::Argument => match stmt {
            // ```python
            // def foo(x: annotation):
            //   ...
            // ```
            //
            // We trust the annotation and see if the type checker matches the annotation.
            Stmt::FunctionDef(ast::StmtFunctionDef { parameters, .. }) => {
                let Some(parameter) = find_parameter_by_name(parameters.as_ref(), name) else {
                    return false;
                };
                let Some(ref annotation) = parameter.parameter.annotation else {
                    return false;
                };
                T::match_annotation(semantic, annotation.as_ref())
            }
            _ => false,
        },

        BindingKind::Annotation => match stmt {
            // ```python
            // x: annotation
            // ```
            //
            // It's a typed declaration, type annotation is the only source of information.
            Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. }) => {
                T::match_annotation(semantic, annotation.as_ref())
            }
            _ => false,
        },

        _ => false,
    }
}

/// Type checker for builtin types.
trait BuiltinTypeChecker {
    /// Builtin type name.
    const BUILTIN_TYPE_NAME: &'static str;
    /// Type name as found in the `Typing` module.
    const TYPING_NAME: &'static str;
    /// [`PythonType`] associated with the intended type.
    const EXPR_TYPE: PythonType;

    /// Check annotation expression to match the intended type.
    fn match_annotation(semantic: &SemanticModel, annotation: &Expr) -> bool {
        let Expr::Subscript(ast::ExprSubscript { value, .. }) = annotation else {
            return false;
        };
        Self::match_builtin_type(semantic, value)
            || semantic.match_typing_expr(value, Self::TYPING_NAME)
    }

    /// Check initializer expression to match the intended type.
    fn match_initializer(semantic: &SemanticModel, initializer: &Expr) -> bool {
        Self::match_expr_type(initializer) || Self::match_builtin_constructor(semantic, initializer)
    }

    /// Check if the type can be inferred from the given expression.
    fn match_expr_type(initializer: &Expr) -> bool {
        let init_type: ResolvedPythonType = initializer.into();
        match init_type {
            ResolvedPythonType::Atom(atom) => atom == Self::EXPR_TYPE,
            _ => false,
        }
    }

    /// Check if the given expression corresponds to a constructor call of the builtin type.
    fn match_builtin_constructor(semantic: &SemanticModel, initializer: &Expr) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = initializer else {
            return false;
        };
        Self::match_builtin_type(semantic, func.as_ref())
    }

    /// Check if the given expression names the builtin type.
    fn match_builtin_type(semantic: &SemanticModel, type_expr: &Expr) -> bool {
        let Expr::Name(ast::ExprName { id, .. }) = type_expr else {
            return false;
        };
        id == Self::BUILTIN_TYPE_NAME && semantic.is_builtin(Self::BUILTIN_TYPE_NAME)
    }
}

impl<T: BuiltinTypeChecker> TypeChecker for T {
    fn match_annotation(semantic: &SemanticModel, annotation: &Expr) -> bool {
        <Self as BuiltinTypeChecker>::match_annotation(semantic, annotation)
    }

    fn match_initializer(semantic: &SemanticModel, initializer: &Expr) -> bool {
        <Self as BuiltinTypeChecker>::match_initializer(semantic, initializer)
    }
}

struct ListChecker;

impl BuiltinTypeChecker for ListChecker {
    const TYPING_NAME: &'static str = "List";
    const BUILTIN_TYPE_NAME: &'static str = "list";
    const EXPR_TYPE: PythonType = PythonType::List;
}

struct DictChecker;

impl BuiltinTypeChecker for DictChecker {
    const TYPING_NAME: &'static str = "Dict";
    const BUILTIN_TYPE_NAME: &'static str = "dict";
    const EXPR_TYPE: PythonType = PythonType::Dict;
}

struct SetChecker;

impl BuiltinTypeChecker for SetChecker {
    const TYPING_NAME: &'static str = "Set";
    const BUILTIN_TYPE_NAME: &'static str = "set";
    const EXPR_TYPE: PythonType = PythonType::Set;
}

/// Test whether the given binding (and the given name) can be considered a list.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `list` and `typing.List`).
pub(super) fn is_list<'a>(semantic: &'a SemanticModel, binding: &'a Binding, name: &str) -> bool {
    check_type::<ListChecker>(semantic, binding, name)
}

/// Test whether the given binding (and the given name) can be considered a dictionary.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `dict` and `typing.Dict`).
pub(super) fn is_dict<'a>(semantic: &'a SemanticModel, binding: &'a Binding, name: &str) -> bool {
    check_type::<DictChecker>(semantic, binding, name)
}

/// Test whether the given binding (and the given name) can be considered a set.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `set` and `typing.Set`).
pub(super) fn is_set<'a>(semantic: &'a SemanticModel, binding: &'a Binding, name: &str) -> bool {
    check_type::<SetChecker>(semantic, binding, name)
}

#[inline]
fn find_parameter_by_name<'a>(
    parameters: &'a Parameters,
    name: &'a str,
) -> Option<&'a ParameterWithDefault> {
    find_parameter_by_name_impl(&parameters.args, name)
        .or_else(|| find_parameter_by_name_impl(&parameters.posonlyargs, name))
        .or_else(|| find_parameter_by_name_impl(&parameters.kwonlyargs, name))
}

#[inline]
fn find_parameter_by_name_impl<'a>(
    parameters: &'a [ParameterWithDefault],
    name: &'a str,
) -> Option<&'a ParameterWithDefault> {
    parameters
        .iter()
        .find(|arg| arg.parameter.name.as_str() == name)
}
