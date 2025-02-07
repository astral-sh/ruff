use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{
    self as ast, Arguments, CmpOp, ElifElseClause, Expr, ExprContext, Identifier, Stmt,
};
use ruff_python_semantic::analyze::typing::{
    is_known_to_be_of_type_dict, is_sys_version_block, is_type_checking_block,
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::fits;

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
/// if "bar" in foo:
///     value = foo["bar"]
/// else:
///     value = 0
/// ```
///
/// Use instead:
/// ```python
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
/// ## References
/// - [Python documentation: Mapping Types](https://docs.python.org/3/library/stdtypes.html#mapping-types-dict)
#[derive(ViolationMetadata)]
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
pub(crate) fn if_else_block_instead_of_dict_get(checker: &Checker, stmt_if: &ast::StmtIf) {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        ..
    } = stmt_if;

    let [body_stmt] = body.as_slice() else {
        return;
    };
    let [ElifElseClause {
        body: else_body,
        test: None,
        ..
    }] = elif_else_clauses.as_slice()
    else {
        return;
    };
    let [else_body_stmt] = else_body.as_slice() else {
        return;
    };
    let Stmt::Assign(ast::StmtAssign {
        targets: body_var,
        value: body_value,
        ..
    }) = &body_stmt
    else {
        return;
    };
    let [body_var] = body_var.as_slice() else {
        return;
    };
    let Stmt::Assign(ast::StmtAssign {
        targets: orelse_var,
        value: orelse_value,
        ..
    }) = &else_body_stmt
    else {
        return;
    };
    let [orelse_var] = orelse_var.as_slice() else {
        return;
    };

    let Expr::Compare(ast::ExprCompare {
        left: test_key,
        ops,
        comparators: test_dict,
        range: _,
    }) = &**test
    else {
        return;
    };
    let [test_dict] = &**test_dict else {
        return;
    };

    if !test_dict
        .as_name_expr()
        .is_some_and(|dict_name| is_known_to_be_of_type_dict(checker.semantic(), dict_name))
    {
        return;
    }

    let (expected_var, expected_value, default_var, default_value) = match ops[..] {
        [CmpOp::In] => (body_var, body_value, orelse_var, orelse_value.as_ref()),
        [CmpOp::NotIn] => (orelse_var, orelse_value, body_var, body_value.as_ref()),
        _ => {
            return;
        }
    };
    let Expr::Subscript(ast::ExprSubscript {
        value: expected_subscript,
        slice: expected_slice,
        ..
    }) = expected_value.as_ref()
    else {
        return;
    };

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

    let node = default_value.clone();
    let node1 = *test_key.clone();
    let node2 = ast::ExprAttribute {
        value: expected_subscript.clone(),
        attr: Identifier::new("get".to_string(), TextRange::default()),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let node3 = ast::ExprCall {
        func: Box::new(node2.into()),
        arguments: Arguments {
            args: Box::from([node1, node]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    let node4 = expected_var.clone();
    let node5 = ast::StmtAssign {
        targets: vec![node4],
        value: Box::new(node3.into()),
        range: TextRange::default(),
    };
    let contents = checker.generator().stmt(&node5.into());

    // Don't flag if the resulting expression would exceed the maximum line length.
    if !fits(
        &contents,
        stmt_if.into(),
        checker.locator(),
        checker.settings.pycodestyle.max_line_length,
        checker.settings.tab_size,
    ) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfDictGet {
            contents: contents.clone(),
        },
        stmt_if.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(stmt_if, checker.source())
    {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            stmt_if.range(),
        )));
    }
    checker.report_diagnostic(diagnostic);
}

/// SIM401
pub(crate) fn if_exp_instead_of_dict_get(
    checker: &Checker,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    let Expr::Compare(ast::ExprCompare {
        left: test_key,
        ops,
        comparators: test_dict,
        range: _,
    }) = test
    else {
        return;
    };
    let [test_dict] = &**test_dict else {
        return;
    };

    let (body, default_value) = match &**ops {
        [CmpOp::In] => (body, orelse),
        [CmpOp::NotIn] => (orelse, body),
        _ => {
            return;
        }
    };

    let Expr::Subscript(ast::ExprSubscript {
        value: expected_subscript,
        slice: expected_slice,
        ..
    }) = body
    else {
        return;
    };

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

    let default_value_node = default_value.clone();
    let dict_key_node = *test_key.clone();
    let dict_get_node = ast::ExprAttribute {
        value: expected_subscript.clone(),
        attr: Identifier::new("get".to_string(), TextRange::default()),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let fixed_node = ast::ExprCall {
        func: Box::new(dict_get_node.into()),
        arguments: Arguments {
            args: Box::from([dict_key_node, default_value_node]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };

    let contents = checker.generator().expr(&fixed_node.into());

    let mut diagnostic = Diagnostic::new(
        IfElseBlockInsteadOfDictGet {
            contents: contents.clone(),
        },
        expr.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(expr, checker.source())
    {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            expr.range(),
        )));
    }
    checker.report_diagnostic(diagnostic);
}
