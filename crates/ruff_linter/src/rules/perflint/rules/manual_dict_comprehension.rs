use crate::checkers::ast::Checker;
use anyhow::{anyhow, Result};
use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::analyze::typing::is_dict;
use ruff_python_semantic::Binding;
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for `for` loops that can be replaced by a dictionary comprehension.
///
/// ## Why is this bad?
/// When creating or extending a dictionary in a for-loop, prefer a dictionary
/// comprehension. Comprehensions are more readable and more performant.
///
/// For example, when comparing `{x: x for x in list(range(1000))}` to the `for`
/// loop version, the comprehension is ~10% faster on Python 3.11.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// pairs = (("a", 1), ("b", 2))
/// result = {}
/// for x, y in pairs:
///     if y % 2:
///         result[x] = y
/// ```
///
/// Use instead:
/// ```python
/// pairs = (("a", 1), ("b", 2))
/// result = {x: y for x, y in pairs if y % 2}
/// ```
///
/// If you're appending to an existing dictionary, use the `update` method instead:
/// ```python
/// pairs = (("a", 1), ("b", 2))
/// result.update({x: y for x, y in pairs if y % 2})
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ManualDictComprehension {
    fix_type: DictComprehensionType,
}

impl Violation for ManualDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        // TODO: async dict comprehensions?
        match self.fix_type {
            DictComprehensionType::Comprehension => {
                "Use a dictionary comprehension instead of a for-loop".to_string()
            }
            DictComprehensionType::Update => "Use `dict.update` instead of a for-loop".to_string(),
        }
    }
}

/// PERF403
pub(crate) fn manual_dict_comprehension(checker: &mut Checker, for_stmt: &ast::StmtFor) {
    let ast::StmtFor { body, target, .. } = for_stmt;
    let body = body.as_slice();
    let target = target.as_ref();
    let (stmt, if_test) = match body {
        // ```python
        // for idx, name in enumerate(names):
        //     if idx % 2 == 0:
        //         result[name] = idx
        // ```
        [Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            test,
            ..
        })] => {
            // TODO(charlie): If there's an `else` clause, verify that the `else` has the
            // same structure.
            if !elif_else_clauses.is_empty() {
                return;
            }
            let [stmt] = body.as_slice() else {
                return;
            };
            (stmt, Some(test))
        }
        // ```python
        // for idx, name in enumerate(names):
        //     result[name] = idx
        // ```
        [stmt] => (stmt, None),
        _ => return,
    };

    let Stmt::Assign(ast::StmtAssign {
        targets,
        value,
        range,
    }) = stmt
    else {
        return;
    };

    let [Expr::Subscript(ast::ExprSubscript {
        value: subscript_value,
        slice,
        ..
    })] = targets.as_slice()
    else {
        return;
    };

    match target {
        Expr::Tuple(tuple) => {
            if !tuple
                .iter()
                .any(|element| ComparableExpr::from(slice) == ComparableExpr::from(element))
            {
                return;
            }
            if !tuple
                .iter()
                .any(|element| ComparableExpr::from(value) == ComparableExpr::from(element))
            {
                return;
            }
        }
        Expr::Name(_) => {
            if ComparableExpr::from(slice) != ComparableExpr::from(target) {
                return;
            }
            if ComparableExpr::from(value) != ComparableExpr::from(target) {
                return;
            }
        }
        _ => return,
    }

    // Exclude non-dictionary value.
    let Expr::Name(name) = &**subscript_value else {
        return;
    };
    let Some(binding) = checker
        .semantic()
        .only_binding(name)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !is_dict(binding, checker.semantic()) {
        return;
    }

    // Avoid if the value is used in the conditional test, e.g.,
    //
    // ```python
    // for x in y:
    //    if x in filtered:
    //        filtered[x] = y
    // ```
    //
    // Converting this to a dictionary comprehension would raise a `NameError` as
    // `filtered` is not defined yet:
    //
    // ```python
    // filtered = {x: y for x in y if x in filtered}
    // ```
    if if_test.is_some_and(|test| {
        any_over_expr(test, &|expr| {
            ComparableExpr::from(expr) == ComparableExpr::from(name)
        })
    }) {
        return;
    }
    let binding_stmt = binding.statement(checker.semantic());
    let binding_value = binding_stmt.and_then(|binding_stmt| match binding_stmt {
        ast::Stmt::AnnAssign(assign) => assign.value.as_deref(),
        ast::Stmt::Assign(assign) => Some(&assign.value),
        _ => None,
    });
    // If the variable is an empty dict literal, then we might be able to replace it with a full dict comprehension.
    // otherwise, it has to be replaced with a `dict.update`
    let binding_is_empty_dict =
        binding_value.is_some_and(|binding_value| match binding_value.as_dict_expr() {
            Some(dict_expr) => dict_expr.is_empty(),
            None => false,
        });
    let assignment_in_same_statement = {
        binding.source.is_some_and(|binding_source| {
            let for_loop_parent = checker.semantic().current_statement_parent_id();
            let binding_parent = checker.semantic().parent_statement_id(binding_source);
            for_loop_parent == binding_parent
        })
    };
    // If the binding is not a single name expression, it could be replaced with a dict comprehension,
    // but not necessarily, so this needs to be manually fixed. This does not apply when using an update.
    let binding_has_one_target = binding_stmt.is_some_and(|binding_stmt| match binding_stmt {
        ast::Stmt::AnnAssign(_) => true,
        ast::Stmt::Assign(assign) => assign.targets.len() == 1,
        _ => false,
    });
    // If the binding gets used in between the assignment and the for loop, a list comprehension is no longer safe

    // If the binding is after the for loop, then it can't be fixed, and this check would panic,
    // so we check that they are in the same statement first
    let binding_unused_between = assignment_in_same_statement
        && binding_stmt.is_some_and(|binding_stmt| {
            let from_assign_to_loop = TextRange::new(binding_stmt.end(), for_stmt.start());
            // Test if there's any reference to the list symbol between its definition and the for loop.
            // if there's at least one, then it's been accessed in the middle somewhere, so it's not safe to change into a list comprehension
            !binding
                .references()
                .map(|ref_id| checker.semantic().reference(ref_id).range())
                .any(|text_range| from_assign_to_loop.contains_range(text_range))
        });
    // A dict update works in every context, while a dict comprehension only works when all the criteria are true
    let fix_type = if binding_is_empty_dict
        && assignment_in_same_statement
        && binding_has_one_target
        && binding_unused_between
    {
        DictComprehensionType::Comprehension
    } else {
        DictComprehensionType::Update
    };

    let mut diagnostic = Diagnostic::new(ManualDictComprehension { fix_type }, *range);

    if checker.settings.preview.is_enabled() {
        // k: v inside the comprehension
        let to_append = (slice.as_ref(), value.as_ref());

        diagnostic.try_set_fix(|| {
            convert_to_dict_comprehension(
                fix_type,
                binding,
                for_stmt,
                if_test.map(std::convert::AsRef::as_ref),
                to_append,
                checker,
            )
        });
    }
    checker.diagnostics.push(diagnostic);
}

fn convert_to_dict_comprehension(
    fix_type: DictComprehensionType,
    binding: &Binding,
    for_stmt: &ast::StmtFor,
    if_test: Option<&ast::Expr>,
    to_append: (&Expr, &Expr),
    checker: &Checker,
) -> Result<Fix> {
    let locator = checker.locator();

    let if_str = match if_test {
        Some(test) => {
            // If the test is an assignment expression,
            // we must parenthesize it when it appears
            // inside the comprehension to avoid a syntax error.
            //
            // Notice that we do not need `any_over_expr` here,
            // since if the assignment expression appears
            // internally (e.g. as an operand in a boolean
            // operation) then it will already be parenthesized.
            if test.is_named_expr() {
                format!(" if ({})", locator.slice(test.range()))
            } else {
                format!(" if {}", locator.slice(test.range()))
            }
        }
        None => String::new(),
    };

    // if the loop target was an implicit tuple, add parentheses around it
    // ```python
    //  for i in a, b:
    //      ...
    // ```
    // becomes
    // [... for i in (a, b)]
    let iter_str = if for_stmt
        .iter
        .as_ref()
        .as_tuple_expr()
        .is_some_and(|expr| !expr.parenthesized)
    {
        format!("({})", locator.slice(for_stmt.iter.range()))
    } else {
        locator.slice(for_stmt.iter.range()).to_string()
    };

    let target_str = locator.slice(for_stmt.target.range());
    let for_type = if for_stmt.is_async {
        "async for"
    } else {
        "for"
    };
    let elt_str = format!(
        "{}: {}",
        locator.slice(to_append.0.range()),
        locator.slice(to_append.1.range())
    );

    let comprehension_str = format!("{{{elt_str} {for_type} {target_str} in {iter_str}{if_str}}}");
    dbg!(&comprehension_str);
    // TODO: handle comments
    match fix_type {
        DictComprehensionType::Update => {
            let variable_name = locator.slice(binding);
            let update_body = format!("{variable_name}.update({comprehension_str})");
            Ok(Fix::unsafe_edit(Edit::range_replacement(
                update_body,
                for_stmt.range,
            )))
        }
        DictComprehensionType::Comprehension => {
            todo!()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DictComprehensionType {
    Update,
    Comprehension,
}
