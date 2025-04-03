use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::name::{QualifiedName, UnqualifiedName};
use ruff_python_ast::{Decorator, Expr, Stmt, StmtExpr, StmtFunctionDef, StmtRaise};

use crate::model::SemanticModel;
use crate::scope::Scope;

#[derive(Debug, Copy, Clone)]
pub enum FunctionType {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
    /// `__new__` is an implicit static method but
    /// is treated similarly to class methods for several lint rules
    NewMethod,
}

/// Classify a function based on its scope, name, and decorators.
pub fn classify(
    name: &str,
    decorator_list: &[Decorator],
    parent_scope: &Scope,
    semantic: &SemanticModel,
    classmethod_decorators: &[String],
    staticmethod_decorators: &[String],
) -> FunctionType {
    if !parent_scope.kind.is_class() {
        return FunctionType::Function;
    }
    if decorator_list
        .iter()
        .any(|decorator| is_static_method(decorator, semantic, staticmethod_decorators))
    {
        FunctionType::StaticMethod
    } else if decorator_list
        .iter()
        .any(|decorator| is_class_method(decorator, semantic, classmethod_decorators))
    {
        FunctionType::ClassMethod
    } else {
        match name {
            "__new__" => FunctionType::NewMethod, // Implicit static method.
            "__init_subclass__" | "__class_getitem__" => FunctionType::ClassMethod, // Implicit class methods.
            _ => FunctionType::Method, // Default to instance method.
        }
    }
}

/// Return `true` if a [`Decorator`] is indicative of a static method.
/// Note: Implicit static methods like `__new__` are not considered.
fn is_static_method(
    decorator: &Decorator,
    semantic: &SemanticModel,
    staticmethod_decorators: &[String],
) -> bool {
    let decorator = map_callable(&decorator.expression);

    // The decorator is an import, so should match against a qualified path.
    if semantic
        .resolve_qualified_name(decorator)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "builtins", "staticmethod"] | ["abc", "abstractstaticmethod"]
            ) || staticmethod_decorators
                .iter()
                .any(|decorator| qualified_name == QualifiedName::from_dotted_name(decorator))
        })
    {
        return true;
    }

    // We do not have a resolvable call path, most likely from a decorator like
    // `@someproperty.setter`. Instead, match on the last element.
    if !staticmethod_decorators.is_empty() {
        if UnqualifiedName::from_expr(decorator).is_some_and(|name| {
            name.segments().last().is_some_and(|tail| {
                staticmethod_decorators
                    .iter()
                    .any(|decorator| tail == decorator)
            })
        }) {
            return true;
        }
    }

    false
}

/// Return `true` if a [`Decorator`] is indicative of a class method.
/// Note: Implicit class methods like `__init_subclass__` and `__class_getitem__` are not considered.
fn is_class_method(
    decorator: &Decorator,
    semantic: &SemanticModel,
    classmethod_decorators: &[String],
) -> bool {
    let decorator = map_callable(&decorator.expression);

    // The decorator is an import, so should match against a qualified path.
    if semantic
        .resolve_qualified_name(decorator)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "builtins", "classmethod"] | ["abc", "abstractclassmethod"]
            ) || classmethod_decorators
                .iter()
                .any(|decorator| qualified_name == QualifiedName::from_dotted_name(decorator))
        })
    {
        return true;
    }

    // We do not have a resolvable call path, most likely from a decorator like
    // `@someproperty.setter`. Instead, match on the last element.
    if !classmethod_decorators.is_empty() {
        if UnqualifiedName::from_expr(decorator).is_some_and(|name| {
            name.segments().last().is_some_and(|tail| {
                classmethod_decorators
                    .iter()
                    .any(|decorator| tail == decorator)
            })
        }) {
            return true;
        }
    }

    false
}

/// Returns `true` if a function has an empty body, and is therefore a stub.
///
/// A function body is considered to be empty if it contains only `pass` statements, `...` literals,
/// `NotImplementedError` raises, or string literal statements (docstrings).
pub fn is_stub(function_def: &StmtFunctionDef, semantic: &SemanticModel) -> bool {
    function_def.body.iter().all(|stmt| match stmt {
        Stmt::Pass(_) => true,
        Stmt::Expr(StmtExpr { value, range: _ }) => {
            matches!(
                value.as_ref(),
                Expr::StringLiteral(_) | Expr::EllipsisLiteral(_)
            )
        }
        Stmt::Raise(StmtRaise {
            range: _,
            exc: exception,
            cause: _,
        }) => exception.as_ref().is_some_and(|exc| {
            semantic
                .resolve_builtin_symbol(map_callable(exc))
                .is_some_and(|name| matches!(name, "NotImplementedError" | "NotImplemented"))
        }),
        _ => false,
    })
}
