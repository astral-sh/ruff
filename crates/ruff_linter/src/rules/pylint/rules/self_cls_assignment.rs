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
#[violation]
pub struct SelfClsAssignment {
    keyword: String,
}

impl Violation for SelfClsAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SelfClsAssignment { keyword } = self;
        format!("Assignment of variable `{keyword}`")
    }
}

/// PLW0127
pub(crate) fn self_cls_assignment(checker: &mut Checker, target: &Expr) {
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

    let keyword = match function_type::classify(
        name,
        decorator_list,
        parent,
        checker.semantic(),
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    ) {
        function_type::FunctionType::ClassMethod { .. } => {
            let Some(first_arg) = parameters.args.first() else {
                return;
            };
            if first_arg.parameter.name.as_str() != "cls" {
                return;
            }

            "cls"
        }
        function_type::FunctionType::Method { .. } => {
            let Some(first_arg) = parameters.args.first() else {
                return;
            };
            if first_arg.parameter.name.as_str() != "self" {
                return;
            }

            "self"
        }
        _ => return,
    };

    check_expr(checker, target, keyword);
}

fn check_expr(checker: &mut Checker, target: &Expr, keyword: &str) {
    match target {
        Expr::Name(_) => {
            if let Expr::Name(ast::ExprName { id, .. }) = target {
                if id.as_str() == keyword {
                    checker.diagnostics.push(Diagnostic::new(
                        SelfClsAssignment {
                            keyword: keyword.to_string(),
                        },
                        target.range(),
                    ));
                }
            }
        }
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            for element in elts {
                check_expr(checker, element, keyword);
            }
        }
        _ => {}
    }
}
