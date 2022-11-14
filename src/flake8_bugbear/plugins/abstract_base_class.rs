use fnv::{FnvHashMap, FnvHashSet};
use rustpython_ast::{Constant, Expr, ExprKind, Keyword, Stmt, StmtKind};

use crate::ast::helpers::{compose_call_path, match_call_path};
use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn is_abc_class(
    bases: &[Expr],
    keywords: &[Keyword],
    from_imports: &FnvHashMap<&str, FnvHashSet<&str>>,
) -> bool {
    keywords.iter().any(|keyword| {
        keyword
            .node
            .arg
            .as_ref()
            .map(|a| a == "metaclass")
            .unwrap_or(false)
            && match_call_path(
                &compose_call_path(&keyword.node.value).unwrap(),
                "abc.ABCMeta",
                from_imports,
            )
    }) || bases.iter().any(|base| {
        compose_call_path(base)
            .map(|call_path| match_call_path(&call_path, "abc.ABC", from_imports))
            .unwrap_or(false)
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

fn is_abstractmethod(expr: &Expr, from_imports: &FnvHashMap<&str, FnvHashSet<&str>>) -> bool {
    compose_call_path(expr)
        .map(|call_path| match_call_path(&call_path, "abc.abstractmethod", from_imports))
        .unwrap_or(false)
}

fn is_overload(expr: &Expr, from_imports: &FnvHashMap<&str, FnvHashSet<&str>>) -> bool {
    compose_call_path(expr)
        .map(|call_path| match_call_path(&call_path, "typing.overload", from_imports))
        .unwrap_or(false)
}

pub fn abstract_base_class(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
    body: &[Stmt],
) {
    if bases.len() + keywords.len() == 1 && is_abc_class(bases, keywords, &checker.from_imports) {
        let mut has_abstract_method = false;
        for stmt in body {
            // https://github.com/PyCQA/flake8-bugbear/issues/293
            // Ignore abc's that declares a class attribute that must be set
            if let StmtKind::AnnAssign { .. } | StmtKind::Assign { .. } = &stmt.node {
                has_abstract_method = true;
                continue;
            }

            if let StmtKind::FunctionDef {
                decorator_list,
                body,
                ..
            }
            | StmtKind::AsyncFunctionDef {
                decorator_list,
                body,
                ..
            } = &stmt.node
            {
                let has_abstract_decorator = decorator_list
                    .iter()
                    .any(|d| is_abstractmethod(d, &checker.from_imports));

                has_abstract_method |= has_abstract_decorator;

                if !has_abstract_decorator
                    && is_empty_body(body)
                    && !decorator_list
                        .iter()
                        .any(|d| is_overload(d, &checker.from_imports))
                {
                    checker.add_check(Check::new(
                        CheckKind::EmptyMethodWithoutAbstractDecorator(name.to_string()),
                        Range::from_located(stmt),
                    ));
                }
            }
        }
        if !has_abstract_method {
            checker.add_check(Check::new(
                CheckKind::AbstractBaseClassWithoutAbstractMethod(name.to_string()),
                Range::from_located(stmt),
            ));
        }
    }
}
