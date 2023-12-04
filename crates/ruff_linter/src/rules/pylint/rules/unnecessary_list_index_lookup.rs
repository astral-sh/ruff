use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, StmtFor};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::SequenceIndexVisitor;

/// ## What it does
/// Checks for index-based list accesses during `enumerate` iterations.
///
/// ## Why is this bad?
/// When iterating over a list with `enumerate`, the current item is already
/// available alongside its index. Using the index to look up the item is
/// unnecessary.
///
/// ## Example
/// ```python
/// letters = ["a", "b", "c"]
///
/// for index, letter in enumerate(letters):
///     print(letters[index])
/// ```
///
/// Use instead:
/// ```python
/// letters = ["a", "b", "c"]
///
/// for index, letter in enumerate(letters):
///     print(letter)
/// ```
#[violation]
pub struct UnnecessaryListIndexLookup;

impl AlwaysFixableViolation for UnnecessaryListIndexLookup {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary lookup of list item by index")
    }

    fn fix_title(&self) -> String {
        format!("Use existing variable")
    }
}

/// PLR1736
pub(crate) fn unnecessary_list_index_lookup(checker: &mut Checker, stmt_for: &StmtFor) {
    let Some((sequence, index_name, value_name)) =
        enumerate_items(&stmt_for.iter, &stmt_for.target, checker.semantic())
    else {
        return;
    };

    let ranges = {
        let mut visitor = SequenceIndexVisitor::new(sequence, index_name, value_name);
        visitor.visit_body(&stmt_for.body);
        visitor.visit_body(&stmt_for.orelse);
        visitor.into_accesses()
    };

    for range in ranges {
        let mut diagnostic = Diagnostic::new(UnnecessaryListIndexLookup, range);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            value_name.to_string(),
            range,
        )));
        checker.diagnostics.push(diagnostic);
    }
}

/// PLR1736
pub(crate) fn unnecessary_list_index_lookup_comprehension(checker: &mut Checker, expr: &Expr) {
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
        let Some((sequence, index_name, value_name)) =
            enumerate_items(&comp.iter, &comp.target, checker.semantic())
        else {
            return;
        };

        let ranges = {
            let mut visitor = SequenceIndexVisitor::new(sequence, index_name, value_name);
            visitor.visit_expr(elt.as_ref());
            visitor.into_accesses()
        };

        for range in ranges {
            let mut diagnostic = Diagnostic::new(UnnecessaryListIndexLookup, range);
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                value_name.to_string(),
                range,
            )));
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn enumerate_items<'a>(
    call_expr: &'a Expr,
    tuple_expr: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a str, &'a str, &'a str)> {
    let ast::ExprCall {
        func, arguments, ..
    } = call_expr.as_call_expr()?;

    // Check that the function is the `enumerate` builtin.
    if !semantic
        .resolve_call_path(func.as_ref())
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["builtins" | "", "enumerate"]))
    {
        return None;
    }

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

    // Get the first argument of the enumerate call.
    let Some(Expr::Name(ast::ExprName { id: sequence, .. })) = arguments.args.first() else {
        return None;
    };

    Some((sequence, index_name, value_name))
}
