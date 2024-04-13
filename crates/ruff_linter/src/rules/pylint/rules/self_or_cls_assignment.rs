use ast::ParameterWithDefault;
use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{analyze::function_type, ScopeKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignment of `self` and `cls` in methods.
///
/// ## Why is this bad?
/// The identifiers `self` and `cls` are conventional in Python for
/// the first argument of instance methods and class methods, respectively.
///
/// ## Example
/// ```python
/// class A:
///     def method(self):
///         self = 1
///     def class_method(cls):
///         cls = 1
/// ```
///
#[violation]
pub struct SelfOrClsAssignment {
    method_type: MethodType,
}

impl Violation for SelfOrClsAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SelfOrClsAssignment { method_type } = self;

        format!(
            "Invalid assignment to `{}` argument in {} method",
            method_type.arg_name(),
            method_type.function_type()
        )
    }
}

/// PLW0127
pub(crate) fn self_or_cls_assignment(checker: &mut Checker, target: &Expr) {
    let ScopeKind::Function(ast::StmtFunctionDef {
        name,
        decorator_list,
        parameters,
        ..
    }) = checker.semantic().current_scope().kind
    else {
        return;
    };

    let Some(parent) = &checker
        .semantic()
        .first_non_type_parent_scope(checker.semantic().current_scope())
    else {
        return;
    };

    let Some(ParameterWithDefault {
        parameter: self_or_cls,
        ..
    }) = parameters
        .posonlyargs
        .first()
        .or_else(|| parameters.args.first())
    else {
        return;
    };

    let function_type = function_type::classify(
        name,
        decorator_list,
        parent,
        checker.semantic(),
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );

    let method_type = match (function_type, self_or_cls.name.as_str()) {
        (function_type::FunctionType::Method { .. }, "self") => MethodType::Instance,
        (function_type::FunctionType::ClassMethod { .. }, "cls") => MethodType::Class,
        _ => return,
    };

    check_expr(checker, target, method_type);
}

fn check_expr(checker: &mut Checker, target: &Expr, method_type: MethodType) {
    match target {
        Expr::Name(_) => {
            if let Expr::Name(ast::ExprName { id, .. }) = target {
                if id.as_str() == method_type.arg_name() {
                    checker.diagnostics.push(Diagnostic::new(
                        SelfOrClsAssignment { method_type },
                        target.range(),
                    ));
                }
            }
        }
        Expr::Tuple(ast::ExprTuple { elts, .. }) | Expr::List(ast::ExprList { elts, .. }) => {
            for element in elts {
                check_expr(checker, element, method_type);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MethodType {
    Instance,
    Class,
}

impl MethodType {
    fn arg_name(self) -> &'static str {
        match self {
            MethodType::Instance => "self",
            MethodType::Class => "cls",
        }
    }

    fn function_type(self) -> &'static str {
        match self {
            MethodType::Instance => "instance",
            MethodType::Class => "class",
        }
    }
}
