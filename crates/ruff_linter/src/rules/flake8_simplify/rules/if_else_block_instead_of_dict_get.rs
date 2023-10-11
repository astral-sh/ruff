use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{
    self as ast, Arguments, CmpOp, ElifElseClause, Expr, ExprContext, Identifier, Stmt,
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
/// ## References
/// - [Python documentation: Mapping Types](https://docs.python.org/3/library/stdtypes.html#mapping-types-dict)
#[violation]
pub struct IfElseBlockInsteadOfDictGet {
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
pub(crate) fn use_dict_get_with_default(checker: &mut Checker, stmt_if: &ast::StmtIf) {
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
    }) = test.as_ref()
    else {
        return;
    };
    let [test_dict] = test_dict.as_slice() else {
        return;
    };
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

    // Check that the default value is not "complex".
    if contains_effect(default_value, |id| checker.semantic().is_builtin(id)) {
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
            args: vec![node1, node],
            keywords: vec![],
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
        checker.settings.line_length,
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
    if !checker.indexer().has_comments(stmt_if, checker.locator()) {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            contents,
            stmt_if.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
