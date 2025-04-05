use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{ArgOrKeyword, CmpOp, Expr, ExprCall, ExprName, Stmt, StmtAssign, StmtIf};
use ruff_python_semantic::analyze::typing::is_known_to_be_of_type_dict;
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for `if key not in dictionary: dictionary[key] = ...` or similar.
///
/// ## Why is this bad?
/// To insert a key-value pair into the dict, when the dict did not have the key present,
/// it's more concise to use `dict.setdefault(key, value)`.
///
/// ## Examples
///
/// ```python
/// if s not in d:
///     d[s] = 3
///
/// if "c" in to_list:
///     to_list["c"].append(3)
/// else:
///     to_list["c"] = [3]
///
///
/// def foo(**kwargs):
///     if "option" not in kwargs:
///         kwargs["option"] = 3
/// ```
///
/// Use instead:
///
/// ```python
/// d.setdefault("c", 3)
///
/// to_list.setdefault("c", []).append(3)
///
///
/// def foo(**kwargs):
///     kwargs.setdefault("option", 3)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe if either:
/// - the statement contains comments
/// - the key expression does not seem idempotent (since after fix the number of key evaluations
///   will be changed)
/// - the value expression does not seem to be cheap to evaluate (since after fix it will
///   be evaluated eagerly)
///
/// ## References
/// - [Python documentation: dict.setdefault](https://docs.python.org/3/library/stdtypes.html#dict.setdefault)
#[derive(ViolationMetadata)]
pub(crate) struct IfKeyNotInDictAssign {
    dict_name: String,
}

impl AlwaysFixableViolation for IfKeyNotInDictAssign {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `.setdefault(...)` instead of check and assign".to_string()
    }

    fn fix_title(&self) -> String {
        let Self { dict_name } = self;
        format!("Replace the statement with `{dict_name}.setdefault(...)")
    }
}

/// RUD060
pub(crate) fn if_key_not_in_dict_assign_via_get(checker: &Checker, assign: &StmtAssign) {
    let Some((dict_a, key_a)) = extract_dict_and_key_from_assign(assign) else {
        return;
    };

    let Some((dict_b, key_b, default)) = extract_dict_key_default_from_get(assign.value.as_ref())
    else {
        return;
    };

    if !is_same_dict(dict_a, dict_b)
        || !is_known_to_be_of_type_dict(checker.semantic(), dict_a)
        || !is_same_key(key_a, key_b.value())
    {
        return;
    }

    let applicability = if checker.comment_ranges().intersects(assign.range) {
        Applicability::Unsafe
    } else if !is_idempotent(checker, key_b.value()) {
        // key will be evaluated once, instead of twice
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    checker.report_diagnostic(
        Diagnostic::new(
            IfKeyNotInDictAssign {
                dict_name: dict_a.id.to_string(),
            },
            assign.range,
        )
        .with_fix(Fix::applicable_edit(
            Edit::range_replacement(
                format!(
                    "{}.setdefault({}{})",
                    dict_a.id,
                    checker.locator().slice(key_b),
                    default
                        .map(|default| format!(", {}", checker.locator().slice(default)))
                        .unwrap_or_default(),
                ),
                assign.range,
            ),
            applicability,
        )),
    );
}

/// RUF060
pub(crate) fn if_key_not_in_dict_assign(checker: &Checker, stmt_if: &StmtIf) {
    let Some((dict_a, key_a, then_assign, else_branch)) = extract_dict_and_key_from_test(stmt_if)
    else {
        return;
    };

    let Some((dict_b, key_b)) = extract_dict_and_key_from_assign(then_assign) else {
        return;
    };

    let key_a_target = if let Expr::Named(named) = key_a {
        named.target.as_ref()
    } else {
        key_a
    };

    if !is_same_dict(dict_a, dict_b)
        || !is_known_to_be_of_type_dict(checker.semantic(), dict_a)
        || !is_same_key(key_a_target, key_b)
    {
        return;
    }

    let assign_value = then_assign.value.as_ref();

    let locator = checker.locator();

    let (default, continuation) = if let Some(else_branch) = else_branch {
        let Some((dict_c, key_c, expr_call)) = extract_dict_key_from_call(else_branch) else {
            return;
        };

        let Some((key_type_a, arg_a)) = extract_key_constructor(assign_value) else {
            return;
        };

        let Some((key_type_b, arg_b, continuation_range)) = extract_key_modifier(expr_call) else {
            return;
        };

        if key_type_a != key_type_b
            || !is_same_dict(dict_a, dict_c)
            || !is_same_key(key_a_target, key_c)
            || !is_same_arg(arg_a, arg_b)
        {
            return;
        }

        (
            key_type_a.default_constructor(),
            locator.slice(continuation_range),
        )
    } else {
        (locator.slice(assign_value), "")
    };

    let applicability = if checker.comment_ranges().intersects(stmt_if.range) {
        Applicability::Unsafe
    } else if else_branch.is_none() && !is_cheap_to_evaluate(assign_value) {
        // `assign_value` will be calculated eagerly (even when key is in dict).
        Applicability::Unsafe
    } else if !is_idempotent(checker, key_b) {
        // key will be evaluated once, instead of twice
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    checker.report_diagnostic(
        Diagnostic::new(
            IfKeyNotInDictAssign {
                dict_name: dict_a.id.to_string(),
            },
            stmt_if.range,
        )
        .with_fix(Fix::applicable_edit(
            Edit::range_replacement(
                format!(
                    "{}.setdefault({}, {default}){continuation}",
                    dict_a.id,
                    locator.slice(key_a),
                ),
                stmt_if.range,
            ),
            applicability,
        )),
    );
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum KeyType {
    List,
    Set,
}

impl KeyType {
    fn default_constructor(self) -> &'static str {
        match self {
            KeyType::List => "[]",
            KeyType::Set => "set()",
        }
    }
}

fn extract_key_modifier(expr_call: &ExprCall) -> Option<(KeyType, &Expr, TextRange)> {
    if expr_call.arguments.len() != 1 {
        return None;
    }

    let arg = expr_call.arguments.find_positional(0)?;

    let Expr::Attribute(expr_attr) = expr_call.func.as_ref() else {
        return None;
    };

    let key_type = match expr_attr.attr.id.as_str() {
        "append" => KeyType::List,
        "add" => KeyType::Set,
        _ => return None,
    };

    Some((
        key_type,
        arg,
        expr_call.range.add_start(expr_attr.value.range().len()),
    ))
}

fn extract_key_constructor(expr: &Expr) -> Option<(KeyType, &Expr)> {
    let (key_type, elts) = match expr {
        Expr::Set(expr_set) => (KeyType::Set, &expr_set.elts),
        Expr::List(expr_list) => (KeyType::List, &expr_list.elts),
        _ => return None,
    };
    match &elts[..] {
        [elt] => Some((key_type, elt)),
        _ => None,
    }
}

fn extract_dict_and_key_from_test(
    stmt_if: &StmtIf,
) -> Option<(&ExprName, &Expr, &StmtAssign, Option<&Stmt>)> {
    let Expr::Compare(compare) = stmt_if.test.as_ref() else {
        return None;
    };
    let [cmp_op @ (CmpOp::In | CmpOp::NotIn)] = compare.ops.as_ref() else {
        return None;
    };
    let [Expr::Name(dict_a)] = compare.comparators.as_ref() else {
        return None;
    };

    let (then_branch, else_branch) = match (stmt_if.elif_else_clauses.as_slice(), cmp_op) {
        ([], CmpOp::NotIn) => (&stmt_if.body, None),
        ([e], CmpOp::NotIn) if e.test.is_none() => (&stmt_if.body, Some(&e.body)),
        ([e], CmpOp::In) if e.test.is_none() => (&e.body, Some(&stmt_if.body)),
        _ => return None,
    };

    let [Stmt::Assign(assign)] = then_branch.as_slice() else {
        return None;
    };

    let else_branch = match else_branch.map(Vec::as_slice) {
        Some([stmt]) => Some(stmt),
        None => None,
        _ => return None,
    };

    Some((dict_a, compare.left.as_ref(), assign, else_branch))
}

fn extract_dict_key_from_call(stmt: &Stmt) -> Option<(&ExprName, &Expr, &ExprCall)> {
    let Stmt::Expr(stmt_expr) = stmt else {
        return None;
    };
    let Expr::Call(expr_call) = stmt_expr.value.as_ref() else {
        return None;
    };
    let Expr::Attribute(expr_attr) = expr_call.func.as_ref() else {
        return None;
    };
    let Expr::Subscript(subscript) = expr_attr.value.as_ref() else {
        return None;
    };
    let Expr::Name(dict) = subscript.value.as_ref() else {
        return None;
    };
    Some((dict, subscript.slice.as_ref(), expr_call))
}

fn extract_dict_key_default_from_get(
    expr: &Expr,
) -> Option<(&ExprName, ArgOrKeyword, Option<ArgOrKeyword>)> {
    let Expr::Call(expr_call) = expr else {
        return None;
    };
    let Expr::Attribute(expr_attr) = expr_call.func.as_ref() else {
        return None;
    };
    let Expr::Name(dict) = expr_attr.value.as_ref() else {
        return None;
    };
    if expr_attr.attr.id != "get" || expr_call.arguments.keywords.len() > 2 {
        return None;
    }
    let key = expr_call.arguments.find_argument("key", 0)?;
    let default = expr_call.arguments.find_argument("default", 1);
    Some((dict, key, default))
}

fn extract_dict_and_key_from_assign(assign: &StmtAssign) -> Option<(&ExprName, &Expr)> {
    let [Expr::Subscript(subscript)] = assign.targets.as_slice() else {
        return None;
    };
    let Expr::Name(dict) = subscript.value.as_ref() else {
        return None;
    };
    Some((dict, subscript.slice.as_ref()))
}

fn is_same_dict(dict_a: &ExprName, dict_b: &ExprName) -> bool {
    dict_a.id == dict_b.id
}

fn is_same_key(key_a: &Expr, key_b: &Expr) -> bool {
    ComparableExpr::from(key_a) == ComparableExpr::from(key_b)
}

fn is_same_arg(arg_a: &Expr, arg_b: &Expr) -> bool {
    ComparableExpr::from(arg_a) == ComparableExpr::from(arg_b)
}

/// Check if `expr` can be evaluated multiple times without changing the result.
fn is_idempotent(checker: &Checker, expr: &Expr) -> bool {
    !contains_effect(expr, |id| checker.semantic().has_builtin_binding(id))
}

fn is_cheap_to_evaluate(expr: &Expr) -> bool {
    expr.is_literal_expr()
        || match expr {
            Expr::Lambda(_) | Expr::Name(_) => true,
            Expr::List(expr_list) if expr_list.is_empty() => true,
            Expr::Set(expr_set) if expr_set.is_empty() => true,
            Expr::Dict(expr_dict) if expr_dict.is_empty() => true,
            Expr::Tuple(expr_tuple) if expr_tuple.is_empty() => true,
            Expr::Call(expr_call) if expr_call.arguments.is_empty() => {
                match expr_call.func.as_ref() {
                    Expr::Name(expr_name) => {
                        matches!(expr_name.id.as_str(), "list" | "set" | "dict" | "tuple")
                    }
                    _ => false,
                }
            }
            _ => false,
        }
}
