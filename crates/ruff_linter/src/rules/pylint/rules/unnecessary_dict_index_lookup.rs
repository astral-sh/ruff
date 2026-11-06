use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, ExprKind, ExprName};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::pylint::helpers::SequenceIndexVisitor;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.12.0", category = Category::Complexity)]
pub(crate) struct UnnecessaryDictIndexLookup;

impl AlwaysFixableViolation for UnnecessaryDictIndexLookup {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary lookup of dictionary value by key".to_string()
    }

    fn fix_title(&self) -> String {
        "Use existing variable".to_string()
    }
}

/// PLR1733
pub(crate) fn unnecessary_dict_index_lookup(checker: &Checker, stmt_for: &ast::StmtForFields<'_>) {
    let Some((dict_name, index_name, value_name)) = dict_items(stmt_for.iter, stmt_for.target)
    else {
        return;
    };

    let ranges = {
        let mut visitor =
            SequenceIndexVisitor::new(dict_name.id(), index_name.id(), value_name.id());
        visitor.visit_body(stmt_for.body);
        visitor.into_accesses()
    };

    for range in ranges {
        let mut diagnostic = checker.report_diagnostic(UnnecessaryDictIndexLookup, range);
        diagnostic.set_fix(Fix::safe_edits(
            Edit::range_replacement(value_name.id().to_string(), range),
            [noop(index_name), noop(value_name)],
        ));
    }
}

/// PLR1733
pub(crate) fn unnecessary_dict_index_lookup_comprehension(checker: &Checker, &expr: &Expr<'_>) {
    let (elt, generators) = match expr.fields() {
        ast::ExprFields::Generator(ast::ExprGeneratorFields {
            elt, generators, ..
        })
        | ast::ExprFields::SetComp(ast::ExprSetCompFields {
            elt, generators, ..
        })
        | ast::ExprFields::ListComp(ast::ExprListCompFields {
            elt, generators, ..
        }) => (elt, generators),
        ast::ExprFields::DictComp(ast::ExprDictCompFields {
            value, generators, ..
        }) => (value, generators),
        _ => return,
    };

    for comp in generators {
        let Some((dict_name, index_name, value_name)) = dict_items(comp.iter(), comp.target())
        else {
            continue;
        };

        let ranges = {
            let mut visitor =
                SequenceIndexVisitor::new(dict_name.id(), index_name.id(), value_name.id());
            visitor.visit_expr(elt);
            for expr in comp.ifs() {
                visitor.visit_expr(expr);
            }
            visitor.into_accesses()
        };

        for range in ranges {
            let mut diagnostic = checker.report_diagnostic(UnnecessaryDictIndexLookup, range);
            diagnostic.set_fix(Fix::safe_edits(
                Edit::range_replacement(value_name.id().to_string(), range),
                [noop(index_name), noop(value_name)],
            ));
        }
    }
}

fn dict_items<'a>(
    call_expr: Expr<'a>,
    tuple_expr: Expr<'a>,
) -> Option<(ExprName<'a>, ExprName<'a>, ExprName<'a>)> {
    let expr_call = call_expr.as_call_expr()?;
    let func = expr_call.func();
    let arguments = expr_call.arguments();

    if !arguments.is_empty() {
        return None;
    }
    let ExprKind::Attribute(attribute) = func.classify() else {
        return None;
    };
    let value = attribute.value();
    let attr = attribute.attr();
    if attr != "items" {
        return None;
    }

    let ExprKind::Name(dict_name) = value.classify() else {
        return None;
    };

    let ExprKind::Tuple(tuple) = tuple_expr.classify() else {
        return None;
    };
    let elts = tuple.elts();
    let [index, value] = elts.as_array()?;

    // Grab the variable names.
    let ExprKind::Name(index_name) = index.classify() else {
        return None;
    };

    let ExprKind::Name(value_name) = value.classify() else {
        return None;
    };

    // If either of the variable names are intentionally ignored by naming them `_`, then don't
    // emit.
    if index_name.id() == "_" || value_name.id() == "_" {
        return None;
    }

    Some((dict_name, index_name, value_name))
}

/// Return a no-op edit for the given name.
fn noop(name: ExprName<'_>) -> Edit {
    Edit::range_replacement(name.id().to_string(), name.range())
}
