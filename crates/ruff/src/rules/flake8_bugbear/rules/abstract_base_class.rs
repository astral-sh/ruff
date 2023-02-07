use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, Rule};
use crate::violation::Violation;
use crate::visibility::{is_abstract, is_overload};

define_violation!(
    pub struct AbstractBaseClassWithoutAbstractMethod {
        pub name: String,
    }
);
impl Violation for AbstractBaseClassWithoutAbstractMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AbstractBaseClassWithoutAbstractMethod { name } = self;
        format!("`{name}` is an abstract base class, but it has no abstract methods")
    }
}
define_violation!(
    pub struct EmptyMethodWithoutAbstractDecorator {
        pub name: String,
    }
);
impl Violation for EmptyMethodWithoutAbstractDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EmptyMethodWithoutAbstractDecorator { name } = self;
        format!(
            "`{name}` is an empty method in an abstract base class, but has no abstract decorator"
        )
    }
}

fn is_abc_class(checker: &Checker, bases: &[Expr], keywords: &[Keyword]) -> bool {
    keywords.iter().any(|keyword| {
        keyword
            .node
            .arg
            .as_ref()
            .map_or(false, |arg| arg == "metaclass")
            && checker
                .resolve_call_path(&keyword.node.value)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["abc", "ABCMeta"]
                })
    }) || bases.iter().any(|base| {
        checker
            .resolve_call_path(base)
            .map_or(false, |call_path| call_path.as_slice() == ["abc", "ABC"])
    })
}

fn is_empty_body(body: &[Stmt]) -> bool {
    body.iter().all(|stmt| match &stmt.node {
        StmtKind::Pass => true,
        StmtKind::Expr { value } => match &value.node {
            ExprKind::Constant { value, .. } => {
                matches!(value, Constant::Str(..) | Constant::Ellipsis)
            }
            _ => false,
        },
        _ => false,
    })
}

pub fn abstract_base_class(
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
    if !is_abc_class(checker, bases, keywords) {
        return;
    }

    let mut has_abstract_method = false;
    for stmt in body {
        // https://github.com/PyCQA/flake8-bugbear/issues/293
        // Ignore abc's that declares a class attribute that must be set
        if let StmtKind::AnnAssign { .. } | StmtKind::Assign { .. } = &stmt.node {
            has_abstract_method = true;
            continue;
        }

        let (
            StmtKind::FunctionDef {
                decorator_list,
                body,
                name: method_name,
                ..
            } | StmtKind::AsyncFunctionDef {
                decorator_list,
                body,
                name: method_name,
                ..
            }
        ) = &stmt.node else {
            continue;
        };

        let has_abstract_decorator = is_abstract(checker, decorator_list);
        has_abstract_method |= has_abstract_decorator;

        if !checker
            .settings
            .rules
            .enabled(&Rule::EmptyMethodWithoutAbstractDecorator)
        {
            continue;
        }

        if !has_abstract_decorator && is_empty_body(body) && !is_overload(checker, decorator_list) {
            checker.diagnostics.push(Diagnostic::new(
                EmptyMethodWithoutAbstractDecorator {
                    name: format!("{name}.{method_name}"),
                },
                Range::from_located(stmt),
            ));
        }
    }
    if checker
        .settings
        .rules
        .enabled(&Rule::AbstractBaseClassWithoutAbstractMethod)
    {
        if !has_abstract_method {
            checker.diagnostics.push(Diagnostic::new(
                AbstractBaseClassWithoutAbstractMethod {
                    name: name.to_string(),
                },
                Range::from_located(stmt),
            ));
        }
    }
}
