use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{self as ast, Builder, CmpOp, Expr, ExprKind, StmtKind};
use ruff_python_semantic::analyze::typing::{
    is_known_to_be_of_type_dict, is_sys_version_block, is_type_checking_block,
};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::fits;
use crate::fix::syntax;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `if` statements that can be replaced with `dict.get` calls.
///
/// ## Why is this bad?
/// `dict.get()` calls can be used to replace `if` statements that assign a
/// value to a variable in both branches, falling back to a default value if
/// the key is not found. When possible, using `dict.get` is more concise and
/// more idiomatic.
///
/// Under [preview mode](https://docs.astral.sh/ruff/preview), this rule will
/// also suggest replacing `if`-`else` _expressions_ with `dict.get` calls.
///
/// ## Example
/// ```python
/// foo = {}
/// if "bar" in foo:
///     value = foo["bar"]
/// else:
///     value = 0
/// ```
///
/// Use instead:
/// ```python
/// foo = {}
/// value = foo.get("bar", 0)
/// ```
///
/// If preview mode is enabled:
/// ```python
/// value = foo["bar"] if "bar" in foo else 0
/// ```
///
/// Use instead:
/// ```python
/// value = foo.get("bar", 0)
/// ```
///
/// ## Options
///
/// The rule will avoid flagging cases where using the resulting `dict.get` call would exceed the
/// configured line length, as determined by these options:
///
/// - `lint.pycodestyle.max-line-length`
/// - `indent-width`
///
/// ## References
/// - [Python documentation: Mapping Types](https://docs.python.org/3/library/stdtypes.html#mapping-types-dict)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.219", category = Category::Complexity)]
pub(crate) struct IfElseBlockInsteadOfDictGet {
    contents: String,
}

impl Violation for IfElseBlockInsteadOfDictGet {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfElseBlockInsteadOfDictGet { contents } = self;
        format!("Use `{contents}` instead of an `if` block")
    }

    fn fix_title(&self) -> Option<String> {
        let IfElseBlockInsteadOfDictGet { contents } = self;
        Some(format!("Replace with `{contents}`"))
    }
}

/// SIM401
pub(crate) fn if_else_block_instead_of_dict_get(checker: &Checker, stmt_if: ast::StmtIf<'_>) {
    let test = stmt_if.test();
    let body = stmt_if.body();
    let elif_else_clauses = stmt_if.elif_else_clauses();

    let Some([body_stmt]) = body.as_array() else {
        return;
    };
    let Some([elif_else_clause]) = elif_else_clauses.as_array() else {
        return;
    };
    let ast::ElifElseClauseFields {
        body: else_body,
        test: None,
        ..
    } = elif_else_clause.fields()
    else {
        return;
    };
    let Some([else_body_stmt]) = else_body.as_array() else {
        return;
    };
    let StmtKind::Assign(assign_node) = body_stmt.classify() else {
        return;
    };
    let body_var = assign_node.targets();
    let body_value = assign_node.value();
    let Some([body_var]) = body_var.as_array() else {
        return;
    };
    let StmtKind::Assign(assign_node) = else_body_stmt.classify() else {
        return;
    };
    let orelse_var = assign_node.targets();
    let orelse_value = assign_node.value();
    let Some([orelse_var]) = orelse_var.as_array() else {
        return;
    };

    let ExprKind::Compare(compare) = test.classify() else {
        return;
    };
    let Some((test_key, op, test_dict)) = compare.as_single() else {
        return;
    };

    if !test_dict
        .as_name_expr()
        .is_some_and(|dict_name| is_known_to_be_of_type_dict(checker.semantic(), dict_name))
    {
        return;
    }

    let (expected_var, expected_value, default_var, default_value) = match op {
        CmpOp::In => (body_var, body_value, orelse_var, orelse_value),
        CmpOp::NotIn => (orelse_var, orelse_value, body_var, body_value),
        _ => {
            return;
        }
    };
    let ExprKind::Subscript(subscript) = expected_value.classify() else {
        return;
    };
    let expected_subscript = subscript.value();
    let expected_slice = subscript.slice();

    // Check that the dictionary key, target variables, and dictionary name are all
    // equivalent.
    if ComparableExpr::from(expected_slice) != ComparableExpr::from(test_key)
        || ComparableExpr::from(expected_var) != ComparableExpr::from(default_var)
        || ComparableExpr::from(test_dict) != ComparableExpr::from(expected_subscript)
    {
        return;
    }

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if is_sys_version_block(stmt_if, checker.semantic()) {
        return;
    }

    // Avoid suggesting ternary for `if TYPE_CHECKING:`-style checks.
    if is_type_checking_block(stmt_if, checker.semantic()) {
        return;
    }

    // Check that the default value is not "complex".
    if contains_effect(default_value, |id| {
        checker.semantic().has_builtin_binding(id)
    }) {
        return;
    }

    let mut builder = Builder::default();
    let dict = builder.copy_node(expected_subscript);
    let args = builder.copy_nodes([test_key, default_value]);
    let value = syntax::call_method(&mut builder, dict, "get", args);
    let assignment = syntax::assignment(&mut builder, expected_var, value);
    let contents = checker.generator().stmt(builder.get(assignment));
    // Don't flag if the resulting expression would exceed the maximum line length.
    if !fits(
        &contents,
        stmt_if.into(),
        checker.locator(),
        checker.settings().pycodestyle.max_line_length,
        checker.settings().tab_size,
    ) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        IfElseBlockInsteadOfDictGet {
            contents: contents.clone(),
        },
        stmt_if.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(&stmt_if, checker.source())
    {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            stmt_if.range(),
        )));
    }
}

/// SIM401
pub(crate) fn if_exp_instead_of_dict_get(
    checker: &Checker,
    &expr: &Expr<'_>,
    test: Expr<'_>,
    body: Expr<'_>,
    orelse: Expr<'_>,
) {
    let ExprKind::Compare(compare) = test.classify() else {
        return;
    };
    let Some((test_key, op, test_dict)) = compare.as_single() else {
        return;
    };

    let (body, default_value) = match op {
        CmpOp::In => (body, orelse),
        CmpOp::NotIn => (orelse, body),
        _ => {
            return;
        }
    };

    let ExprKind::Subscript(subscript) = body.classify() else {
        return;
    };
    let expected_subscript = subscript.value();
    let expected_slice = subscript.slice();

    if ComparableExpr::from(expected_slice) != ComparableExpr::from(test_key)
        || ComparableExpr::from(test_dict) != ComparableExpr::from(expected_subscript)
    {
        return;
    }

    // Check that the default value is not "complex".
    if contains_effect(default_value, |id| {
        checker.semantic().has_builtin_binding(id)
    }) {
        return;
    }

    let mut builder = Builder::default();
    let dict = builder.copy_node(expected_subscript);
    let args = builder.copy_nodes([test_key, default_value]);
    let replacement = syntax::call_method(&mut builder, dict, "get", args);
    let contents = checker.generator().expr(builder.get(replacement));
    let mut diagnostic = checker.report_diagnostic(
        IfElseBlockInsteadOfDictGet {
            contents: contents.clone(),
        },
        expr.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(&expr, checker.source())
    {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            expr.range(),
        )));
    }
}
