use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::function_type::{self as function_type, FunctionType};
use ruff_python_semantic::ScopeKind;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignment of `self` and `cls` in instance and class methods respectively.
///
/// This check also applies to `__new__` even though this is technically
/// a static method.
///
/// ## Why is this bad?
/// The identifiers `self` and `cls` are conventional in Python for the first parameter of instance
/// methods and class methods, respectively. Assigning new values to these variables can be
/// confusing for others reading your code; using a different variable name can lead to clearer
/// code.
///
/// ## Example
///
/// ```python
/// class Version:
///     def add(self, other):
///         self = self + other
///         return self
///
///     @classmethod
///     def superclass(cls):
///         cls = cls.__mro__[-1]
///         return cls
/// ```
///
/// Use instead:
/// ```python
/// class Version:
///     def add(self, other):
///         new_version = self + other
///         return new_version
///
///     @classmethod
///     def superclass(cls):
///         supercls = cls.__mro__[-1]
///         return supercls
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SelfOrClsAssignment {
    method_type: MethodType,
}

impl Violation for SelfOrClsAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SelfOrClsAssignment { method_type } = self;

        format!(
            "Reassigned `{}` variable in {method_type} method",
            method_type.arg_name(),
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Consider using a different variable name".to_string())
    }
}

/// PLW0127
pub(crate) fn self_or_cls_assignment(checker: &Checker, target: &Expr) {
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

    let Some(self_or_cls) = parameters
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

    let method_type = match (function_type, self_or_cls.name().as_str()) {
        (FunctionType::Method, "self") => MethodType::Instance,
        (FunctionType::ClassMethod, "cls") => MethodType::Class,
        (FunctionType::NewMethod, "cls") => MethodType::New,
        _ => return,
    };

    check_expr(checker, target, method_type);
}

fn check_expr(checker: &Checker, target: &Expr, method_type: MethodType) {
    match target {
        Expr::Name(_) => {
            if let Expr::Name(ast::ExprName { id, .. }) = target {
                if id.as_str() == method_type.arg_name() {
                    checker.report_diagnostic(Diagnostic::new(
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
        Expr::Starred(ast::ExprStarred { value, .. }) => check_expr(checker, value, method_type),
        _ => {}
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MethodType {
    Instance,
    Class,
    New,
}

impl MethodType {
    const fn arg_name(self) -> &'static str {
        match self {
            MethodType::Instance => "self",
            MethodType::Class => "cls",
            MethodType::New => "cls",
        }
    }
}

impl std::fmt::Display for MethodType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MethodType::Instance => f.write_str("instance"),
            MethodType::Class => f.write_str("class"),
            MethodType::New => f.write_str("`__new__`"),
        }
    }
}
