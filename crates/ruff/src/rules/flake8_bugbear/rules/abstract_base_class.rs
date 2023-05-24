use rustpython_parser::ast::{self, Constant, Expr, Keyword, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::visibility::{is_abstract, is_overload};
use ruff_python_semantic::model::SemanticModel;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[violation]
pub struct AbstractBaseClassWithoutAbstractMethod {
    name: String,
}

impl Violation for AbstractBaseClassWithoutAbstractMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AbstractBaseClassWithoutAbstractMethod { name } = self;
        format!("`{name}` is an abstract base class, but it has no abstract methods")
    }
}
#[violation]
pub struct EmptyMethodWithoutAbstractDecorator {
    name: String,
}

impl Violation for EmptyMethodWithoutAbstractDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EmptyMethodWithoutAbstractDecorator { name } = self;
        format!(
            "`{name}` is an empty method in an abstract base class, but has no abstract decorator"
        )
    }
}

fn is_abc_class(model: &SemanticModel, bases: &[Expr], keywords: &[Keyword]) -> bool {
    keywords.iter().any(|keyword| {
        keyword.arg.as_ref().map_or(false, |arg| arg == "metaclass")
            && model
                .resolve_call_path(&keyword.value)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["abc", "ABCMeta"]
                })
    }) || bases.iter().any(|base| {
        model
            .resolve_call_path(base)
            .map_or(false, |call_path| call_path.as_slice() == ["abc", "ABC"])
    })
}

fn is_empty_body(body: &[Stmt]) -> bool {
    body.iter().all(|stmt| match stmt {
        Stmt::Pass(_) => true,
        Stmt::Expr(ast::StmtExpr {
            value,
            range: _range,
        }) => match value.as_ref() {
            Expr::Constant(ast::ExprConstant { value, .. }) => {
                matches!(value, Constant::Str(..) | Constant::Ellipsis)
            }
            _ => false,
        },
        _ => false,
    })
}

/// B024
/// B027
pub(crate) fn abstract_base_class(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
    body: &[Stmt],
) {
    if bases.len() + keywords.len() != 1 {
        return;
    }
    if !is_abc_class(checker.semantic_model(), bases, keywords) {
        return;
    }

    let mut has_abstract_method = false;
    for stmt in body {
        // https://github.com/PyCQA/flake8-bugbear/issues/293
        // Ignore abc's that declares a class attribute that must be set
        if let Stmt::AnnAssign(_) | Stmt::Assign(_) = stmt {
            has_abstract_method = true;
            continue;
        }

        let (
            Stmt::FunctionDef(ast::StmtFunctionDef {
                decorator_list,
                body,
                name: method_name,
                ..
            }) | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                decorator_list,
                body,
                name: method_name,
                ..
            })
        ) = stmt else {
            continue;
        };

        let has_abstract_decorator = is_abstract(checker.semantic_model(), decorator_list);
        has_abstract_method |= has_abstract_decorator;

        if !checker.enabled(Rule::EmptyMethodWithoutAbstractDecorator) {
            continue;
        }

        if !has_abstract_decorator
            && is_empty_body(body)
            && !is_overload(checker.semantic_model(), decorator_list)
        {
            checker.diagnostics.push(Diagnostic::new(
                EmptyMethodWithoutAbstractDecorator {
                    name: format!("{name}.{method_name}"),
                },
                stmt.range(),
            ));
        }
    }
    if checker.enabled(Rule::AbstractBaseClassWithoutAbstractMethod) {
        if !has_abstract_method {
            checker.diagnostics.push(Diagnostic::new(
                AbstractBaseClassWithoutAbstractMethod {
                    name: name.to_string(),
                },
                stmt.range(),
            ));
        }
    }
}
