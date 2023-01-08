use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Constant, Expr, ExprKind, Keyword, Stmt, StmtKind};

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

fn is_abc_class(
    bases: &[Expr],
    keywords: &[Keyword],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    keywords.iter().any(|keyword| {
        keyword
            .node
            .arg
            .as_ref()
            .map_or(false, |a| a == "metaclass")
            && match_module_member(
                &keyword.node.value,
                "abc",
                "ABCMeta",
                from_imports,
                import_aliases,
            )
    }) || bases
        .iter()
        .any(|base| match_module_member(base, "abc", "ABC", from_imports, import_aliases))
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

fn is_abstractmethod(
    expr: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    match_module_member(expr, "abc", "abstractmethod", from_imports, import_aliases)
}

fn is_overload(
    expr: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    match_module_member(expr, "typing", "overload", from_imports, import_aliases)
}

pub fn abstract_base_class(
    xxxxxxxx: &mut xxxxxxxx,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
    body: &[Stmt],
) {
    if bases.len() + keywords.len() != 1 {
        return;
    }
    if !is_abc_class(
        bases,
        keywords,
        &xxxxxxxx.from_imports,
        &xxxxxxxx.import_aliases,
    ) {
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

        let (StmtKind::FunctionDef {
                decorator_list,
                body,
                ..
            }
            | StmtKind::AsyncFunctionDef {
                decorator_list,
                body,
                ..
            }) = &stmt.node else {
            continue;
        };

        let has_abstract_decorator = decorator_list
            .iter()
            .any(|d| is_abstractmethod(d, &xxxxxxxx.from_imports, &xxxxxxxx.import_aliases));

        has_abstract_method |= has_abstract_decorator;

        if !xxxxxxxx.settings.enabled.contains(&RuleCode::B027) {
            continue;
        }

        if !has_abstract_decorator
            && is_empty_body(body)
            && !decorator_list
                .iter()
                .any(|d| is_overload(d, &xxxxxxxx.from_imports, &xxxxxxxx.import_aliases))
        {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::EmptyMethodWithoutAbstractDecorator(name.to_string()),
                Range::from_located(stmt),
            ));
        }
    }
    if xxxxxxxx.settings.enabled.contains(&RuleCode::B024) {
        if !has_abstract_method {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::AbstractBaseClassWithoutAbstractMethod(name.to_string()),
                Range::from_located(stmt),
            ));
        }
    }
}
