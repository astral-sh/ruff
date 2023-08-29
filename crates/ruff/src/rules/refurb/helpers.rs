use ast::{ParameterWithDefault, Parameters};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::{Binding, BindingKind, SemanticModel};

/// Abstraction for a type checker, conservatively checks for the intended type(s).
trait TypeChecker {
    /// Check annotation expression to match the intended type(s).
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool;
    /// Check initializer expression to match the intended type(s).
    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool;
}

/// Check if the type checker accepts the given binding with the given name.
///
/// NOTE: this function doesn't perform more serious type inference, so it won't be able
///       to understand if the value gets initialized from a call to a function always returning
///       lists. This also implies no interfile analysis.
fn check_type<T: TypeChecker>(binding: &Binding, name: &str, semantic: &SemanticModel) -> bool {
    match binding.kind {
        BindingKind::Assignment => match binding.statement(semantic) {
            // ```python
            // x = init_expr
            // ```
            //
            // The type checker might know how to infer the type based on `init_expr`.
            Some(Stmt::Assign(ast::StmtAssign { value, .. })) => {
                T::match_initializer(value.as_ref(), semantic)
            }

            // ```python
            // x: annotation = some_expr
            // ```
            //
            // In this situation, we check only the annotation.
            Some(Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. })) => {
                T::match_annotation(annotation.as_ref(), semantic)
            }
            _ => false,
        },

        BindingKind::Argument => match binding.statement(semantic) {
            // ```python
            // def foo(x: annotation):
            //   ...
            // ```
            //
            // We trust the annotation and see if the type checker matches the annotation.
            Some(Stmt::FunctionDef(ast::StmtFunctionDef { parameters, .. })) => {
                // TODO(charlie): Store a pointer to the argument in the binding.
                let Some(parameter) = find_parameter_by_name(parameters.as_ref(), name) else {
                    return false;
                };
                let Some(ref annotation) = parameter.parameter.annotation else {
                    return false;
                };
                T::match_annotation(annotation.as_ref(), semantic)
            }
            _ => false,
        },

        BindingKind::Annotation => match binding.statement(semantic) {
            // ```python
            // x: annotation
            // ```
            //
            // It's a typed declaration, type annotation is the only source of information.
            Some(Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. })) => {
                T::match_annotation(annotation.as_ref(), semantic)
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
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        let value = map_subscript(annotation);
        Self::match_builtin_type(value, semantic)
            || semantic.match_typing_expr(value, Self::TYPING_NAME)
    }

    /// Check initializer expression to match the intended type.
    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        Self::match_expr_type(initializer) || Self::match_builtin_constructor(initializer, semantic)
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
    fn match_builtin_constructor(initializer: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = initializer else {
            return false;
        };
        Self::match_builtin_type(func.as_ref(), semantic)
    }

    /// Check if the given expression names the builtin type.
    fn match_builtin_type(type_expr: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Name(ast::ExprName { id, .. }) = type_expr else {
            return false;
        };
        id == Self::BUILTIN_TYPE_NAME && semantic.is_builtin(Self::BUILTIN_TYPE_NAME)
    }
}

impl<T: BuiltinTypeChecker> TypeChecker for T {
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        <Self as BuiltinTypeChecker>::match_annotation(annotation, semantic)
    }

    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        <Self as BuiltinTypeChecker>::match_initializer(initializer, semantic)
    }
}

struct ListChecker;

impl BuiltinTypeChecker for ListChecker {
    const BUILTIN_TYPE_NAME: &'static str = "list";
    const TYPING_NAME: &'static str = "List";
    const EXPR_TYPE: PythonType = PythonType::List;
}

struct DictChecker;

impl BuiltinTypeChecker for DictChecker {
    const BUILTIN_TYPE_NAME: &'static str = "dict";
    const TYPING_NAME: &'static str = "Dict";
    const EXPR_TYPE: PythonType = PythonType::Dict;
}

struct SetChecker;

impl BuiltinTypeChecker for SetChecker {
    const BUILTIN_TYPE_NAME: &'static str = "set";
    const TYPING_NAME: &'static str = "Set";
    const EXPR_TYPE: PythonType = PythonType::Set;
}

/// Test whether the given binding (and the given name) can be considered a list.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `list` and `typing.List`).
pub(super) fn is_list<'a>(binding: &'a Binding, name: &str, semantic: &'a SemanticModel) -> bool {
    check_type::<ListChecker>(binding, name, semantic)
}

/// Test whether the given binding (and the given name) can be considered a dictionary.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `dict` and `typing.Dict`).
pub(super) fn is_dict<'a>(binding: &'a Binding, name: &str, semantic: &'a SemanticModel) -> bool {
    check_type::<DictChecker>(binding, name, semantic)
}

/// Test whether the given binding (and the given name) can be considered a set.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `set` and `typing.Set`).
pub(super) fn is_set<'a>(binding: &'a Binding, name: &str, semantic: &'a SemanticModel) -> bool {
    check_type::<SetChecker>(binding, name, semantic)
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
