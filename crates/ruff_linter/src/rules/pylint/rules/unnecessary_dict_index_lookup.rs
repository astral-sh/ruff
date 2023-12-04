use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, StmtFor};

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::SequenceIndexVisitor;

/// ## What it does
/// Checks for key-based dict accesses during `.items()` iterations.
///
/// ## Why is this bad?
/// When iterating over a dict via `.items()`, the current value is already
/// available alongside its key. Using the key to look up the value is
/// unnecessary.
///
/// ## Example
/// ```python
/// FRUITS = {"apple": 1, "orange": 10, "berry": 22}
///
/// for fruit_name, fruit_count in FRUITS.items():
///     print(FRUITS[fruit_name])
/// ```
///
/// Use instead:
/// ```python
/// FRUITS = {"apple": 1, "orange": 10, "berry": 22}
///
/// for fruit_name, fruit_count in FRUITS.items():
///     print(fruit_count)
/// ```
#[violation]
pub struct UnnecessaryDictIndexLookup;

impl AlwaysFixableViolation for UnnecessaryDictIndexLookup {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary lookup of dictionary value by key")
    }

    fn fix_title(&self) -> String {
        format!("Use existing variable")
    }
}

/// PLR1733
pub(crate) fn unnecessary_dict_index_lookup(checker: &mut Checker, stmt_for: &StmtFor) {
    let Some((dict_name, index_name, value_name)) = dict_items(&stmt_for.iter, &stmt_for.target)
    else {
        return;
    };

    let ranges = {
        let mut visitor = SequenceIndexVisitor::new(dict_name, index_name, value_name);
        visitor.visit_body(&stmt_for.body);
        visitor.visit_body(&stmt_for.orelse);
        visitor.into_accesses()
    };

    for range in ranges {
        let mut diagnostic = Diagnostic::new(UnnecessaryDictIndexLookup, range);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            value_name.to_string(),
            range,
        )));
        checker.diagnostics.push(diagnostic);
    }
}

/// PLR1733
pub(crate) fn unnecessary_dict_index_lookup_comprehension(checker: &mut Checker, expr: &Expr) {
    let (Expr::GeneratorExp(ast::ExprGeneratorExp {
        elt, generators, ..
    })
    | Expr::DictComp(ast::ExprDictComp {
        value: elt,
        generators,
        ..
    })
    | Expr::SetComp(ast::ExprSetComp {
        elt, generators, ..
    })
    | Expr::ListComp(ast::ExprListComp {
        elt, generators, ..
    })) = expr
    else {
        return;
    };

    for comp in generators {
        let Some((dict_name, index_name, value_name)) = dict_items(&comp.iter, &comp.target) else {
            continue;
        };

        let ranges = {
            let mut visitor = SequenceIndexVisitor::new(dict_name, index_name, value_name);
            visitor.visit_expr(elt.as_ref());
            for expr in &comp.ifs {
                visitor.visit_expr(expr);
            }
            visitor.into_accesses()
        };

        for range in ranges {
            let mut diagnostic = Diagnostic::new(UnnecessaryDictIndexLookup, range);
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                value_name.to_string(),
                range,
            )));
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn dict_items<'a>(
    call_expr: &'a Expr,
    tuple_expr: &'a Expr,
) -> Option<(&'a str, &'a str, &'a str)> {
    let ast::ExprCall {
        func, arguments, ..
    } = call_expr.as_call_expr()?;

    if !arguments.is_empty() {
        return None;
    }
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return None;
    };
    if attr != "items" {
        return None;
    }

    let Expr::Name(ast::ExprName { id: dict_name, .. }) = value.as_ref() else {
        return None;
    };

    let Expr::Tuple(ast::ExprTuple { elts, .. }) = tuple_expr else {
        return None;
    };
    let [index, value] = elts.as_slice() else {
        return None;
    };

    // Grab the variable names.
    let Expr::Name(ast::ExprName { id: index_name, .. }) = index else {
        return None;
    };

    let Expr::Name(ast::ExprName { id: value_name, .. }) = value else {
        return None;
    };

    // If either of the variable names are intentionally ignored by naming them `_`, then don't
    // emit.
    if index_name == "_" || value_name == "_" {
        return None;
    }

    Some((dict_name, index_name, value_name))
}
