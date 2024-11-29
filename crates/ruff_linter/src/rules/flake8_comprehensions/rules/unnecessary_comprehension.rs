use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Comprehension, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary `dict`, `list`, and `set` comprehension.
///
/// ## Why is this bad?
/// It's unnecessary to use a `dict`/`list`/`set` comprehension to build a data structure if the
/// elements are unchanged. Wrap the iterable with `dict()`, `list()`, or `set()` instead.
///
/// ## Examples
/// ```python
/// {a: b for a, b in iterable}
/// [x for x in iterable]
/// {x for x in iterable}
/// ```
///
/// Use instead:
/// ```python
/// dict(iterable)
/// list(iterable)
/// set(iterable)
/// ```
///
/// ## Known problems
///
/// This rule may produce false positives for dictionary comprehensions that iterate over a mapping.
/// The `dict` constructor behaves differently depending on if it receives a sequence (e.g., a
/// `list`) or a mapping (e.g., a `dict`). When a comprehension iterates over the keys of a mapping,
/// replacing it with a `dict` constructor call will give a different result.
///
/// For example:
///
/// ```pycon
/// >>> d1 = {(1, 2): 3, (4, 5): 6}
/// >>> {x: y for x, y in d1}  # Iterates over the keys of a mapping
/// {1: 2, 4: 5}
/// >>> dict(d1)               # Ruff's incorrect suggested fix
/// (1, 2): 3, (4, 5): 6}
/// >>> dict(d1.keys())        # Correct fix
/// {1: 2, 4: 5}
/// ```
///
/// When the comprehension iterates over a sequence, Ruff's suggested fix is correct. However, Ruff
/// cannot consistently infer if the iterable type is a sequence or a mapping and cannot suggest
/// the correct fix for mappings.
///
/// ## Fix safety
/// Due to the known problem with dictionary comprehensions, this fix is marked as unsafe.
///
/// Additionally, this fix may drop comments when rewriting the comprehension.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryComprehension {
    obj_type: String,
}

impl AlwaysFixableViolation for UnnecessaryComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryComprehension { obj_type } = self;
        format!("Unnecessary `{obj_type}` comprehension (rewrite using `{obj_type}()`)")
    }

    fn fix_title(&self) -> String {
        let UnnecessaryComprehension { obj_type } = self;
        format!("Rewrite using `{obj_type}()`")
    }
}

/// Add diagnostic for C416 based on the expression node id.
fn add_diagnostic(checker: &mut Checker, expr: &Expr) {
    let id = match expr {
        Expr::ListComp(_) => "list",
        Expr::SetComp(_) => "set",
        Expr::DictComp(_) => "dict",
        _ => return,
    };
    if !checker.semantic().has_builtin_binding(id) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryComprehension {
            obj_type: id.to_string(),
        },
        expr.range(),
    );
    diagnostic.try_set_fix(|| {
        fixes::fix_unnecessary_comprehension(expr, checker.locator(), checker.stylist())
            .map(Fix::unsafe_edit)
    });
    checker.diagnostics.push(diagnostic);
}

/// C416
pub(crate) fn unnecessary_dict_comprehension(
    checker: &mut Checker,
    expr: &Expr,
    key: &Expr,
    value: &Expr,
    generators: &[Comprehension],
) {
    let [generator] = generators else {
        return;
    };
    if !generator.ifs.is_empty() || generator.is_async {
        return;
    }
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = &generator.target else {
        return;
    };
    let [target_key, target_value] = elts.as_slice() else {
        return;
    };
    if ComparableExpr::from(key) != ComparableExpr::from(target_key) {
        return;
    }
    if ComparableExpr::from(value) != ComparableExpr::from(target_value) {
        return;
    }
    add_diagnostic(checker, expr);
}

/// C416
pub(crate) fn unnecessary_list_set_comprehension(
    checker: &mut Checker,
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) {
    let [generator] = generators else {
        return;
    };
    if !generator.ifs.is_empty() || generator.is_async {
        return;
    }
    if ComparableExpr::from(elt) != ComparableExpr::from(&generator.target) {
        return;
    }
    add_diagnostic(checker, expr);
}
