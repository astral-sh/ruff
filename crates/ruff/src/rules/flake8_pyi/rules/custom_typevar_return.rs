use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::{ArgWithDefault, Arguments, Decorator, Expr, Ranged, Stmt};
use ruff_python_semantic::analyze::visibility::{
    is_abstract, is_classmethod, is_overload, is_staticmethod,
};
use ruff_python_semantic::ScopeKind;

// TODO: Check docs for accuracy
/// ## What it does
/// Checks if certain methods define a custom `TypeVar`s for their return annotation instead of
/// using `typing_extensions.Self`. This check currently applies for instance methods that return
/// `self`, class methods that return an instance of `cls`, and `__new__` methods.
///
/// ## Why is this bad?
/// If certain methods are annotated with a custom `TypeVar` type, and the class is subclassed,
/// type checkers will not be able to infer the correct return type.
///
/// ## Example
/// ```python
/// class Foo:
///     def __new__(cls: type[_S], *args: str, **kwargs: int) -> _S:
///         ...
///
///     def foo(self: _S, arg: bytes) -> _S:
///         ...
///
///     @classmethod
///     def bar(cls: type[_S], arg: int) -> _S:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// from typing import Self
///
///
/// class Foo:
///     def __new__(cls: type[Self], *args: str, **kwargs: int) -> Self:
///         ...
///
///     def foo(self: Self, arg: bytes) -> Self:
///         ...
///
///     @classmethod
///     def bar(cls: type[Self], arg: int) -> Self:
///         ...
/// ```
#[violation]
pub struct CustomTypeVarReturnType {
    method_name: String,
}

impl Violation for CustomTypeVarReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CustomTypeVarReturnType { method_name } = self;
        format!(
            "Methods like {method_name} should return `typing.Self` instead of a custom TypeVar"
        )
    }
}

/// PYI019
pub(crate) fn custom_typevar_return_type(
    checker: &mut Checker,
    name: &str,
    decorator_list: &[Decorator],
    returns: Option<&Expr>,
    args: &Arguments,
) {
    let ScopeKind::Class(_) = checker.semantic().scope().kind else {
        return;
    };

    if args.args.is_empty() && args.posonlyargs.is_empty() {
        return;
    }

    let Some(returns) = returns else {
        return;
    };

    let return_annotation = if let Expr::Subscript(ast::ExprSubscript { value, .. }) = returns {
        // Ex) `Type[T]`
        value
    } else {
        // Ex) `Type`, `typing.Type`
        returns
    };

    // Skip any abstract, static and overloaded methods.
    if is_abstract(decorator_list, checker.semantic())
        || is_overload(decorator_list, checker.semantic())
        || is_staticmethod(decorator_list, checker.semantic())
    {
        return;
    }

    let is_violation: bool =
        if is_classmethod(decorator_list, checker.semantic()) || name == "__new__" {
            check_class_method_for_bad_typevars(checker, args, return_annotation)
        } else {
            // If not static, or a class method or __new__ we know it is an instance method
            check_instance_method_for_bad_typevars(checker, args, return_annotation)
        };

    if is_violation {
        checker.diagnostics.push(Diagnostic::new(
            CustomTypeVarReturnType {
                method_name: name.to_string(),
            },
            return_annotation.range(),
        ));
    }
}

fn check_class_method_for_bad_typevars(
    checker: &Checker,
    args: &Arguments,
    return_annotation: &Expr,
) -> bool {
    let ArgWithDefault { def, .. } = &args.args[0];

    let Some(annotation) = &def.annotation else {
        return false
    };

    let Expr::Subscript(ast::ExprSubscript{slice, value, ..}) = annotation.as_ref() else {
        return false
    };

    // Don't error if the first argument is annotated with `builtins.type[T]` or `typing.Type[T]`
    // These are edge cases, and it's hard to give good error messages for them.
    if let Expr::Name(ast::ExprName { id: id_value, .. }) = value.as_ref() {
        return id_value == "type";
    }

    let Expr::Name(ast::ExprName { id: id_slice, .. }) = slice.as_ref() else {
        return false
    };

    let Expr::Name(ast::ExprName { id: return_type, .. }) = return_annotation else {
        return false
    };

    return_type == id_slice && is_likely_private_typevar(checker, args, id_slice)
}

fn check_instance_method_for_bad_typevars(
    checker: &Checker,
    args: &Arguments,
    return_annotation: &Expr,
) -> bool {
    let ArgWithDefault { def, .. } = &args.args[0];

    let Some(annotation) = &def.annotation else {
        return false
    };

    let Expr::Name(ast::ExprName{id: first_arg_type,..}) = annotation.as_ref()  else {
        return false
    };

    let Expr::Name(ast::ExprName { id: return_type, .. }) = return_annotation else {
        return false
    };

    if first_arg_type != return_type {
        return false;
    }

    is_likely_private_typevar(checker, args, first_arg_type)
}

fn is_likely_private_typevar(checker: &Checker, args: &Arguments, tvar_name: &str) -> bool {
    if tvar_name.starts_with('_') {
        return true;
    }
    if checker.settings.target_version < PythonVersion::Py312 {
        return false;
    }
    for ArgWithDefault { def, .. } in &args.args {
        if def.arg.to_string() != tvar_name {
            continue;
        }

        let Some(annotation) = &def.annotation else {
            continue
        };

        let Expr::Name(ast::ExprName{id,..}) = annotation.as_ref() else {
            continue
        };

        // Check if the binding used in an annotation is an assignment to typing.TypeVar
        let scope = checker.semantic().scope();
        let Some(binding_id) = scope.get(id) else {
            continue
        };

        let binding = checker.semantic().binding(binding_id);
        if binding.kind.is_assignment() || binding.kind.is_named_expr_assignment() {
            if let Some(parent_id) = binding.source {
                let parent = checker.semantic().stmts[parent_id];
                if let Stmt::Assign(ast::StmtAssign { value, .. })
                | Stmt::AnnAssign(ast::StmtAnnAssign {
                    value: Some(value), ..
                })
                | Stmt::AugAssign(ast::StmtAugAssign { value, .. }) = parent
                {
                    let Expr::Call(ast::ExprCall{func, ..}) = value.as_ref() else {
                            continue
                        };
                    let Some(call_path) = checker.semantic().resolve_call_path(func) else {
                            continue
                        };
                    if checker
                        .semantic()
                        .match_typing_call_path(&call_path, "TypeVar")
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}
