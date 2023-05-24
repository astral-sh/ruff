use std::borrow::Cow;

use anyhow::{bail, Result};
use libcst_native::{
    BooleanOp, BooleanOperation, Codegen, CodegenState, CompoundStatement, Expression, If,
    LeftParen, ParenthesizableWhitespace, ParenthesizedNode, RightParen, SimpleWhitespace,
    Statement, Suite,
};
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::Edit;
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::whitespace;

use crate::cst::matchers::{match_function_def, match_if, match_indented_block, match_statement};

fn parenthesize_and_operand(expr: Expression) -> Expression {
    match &expr {
        _ if !expr.lpar().is_empty() => expr,
        Expression::BooleanOperation(boolean_operation)
            if matches!(boolean_operation.operator, BooleanOp::Or { .. }) =>
        {
            expr.with_parens(LeftParen::default(), RightParen::default())
        }
        Expression::IfExp(_) | Expression::Lambda(_) | Expression::NamedExpr(_) => {
            expr.with_parens(LeftParen::default(), RightParen::default())
        }
        _ => expr,
    }
}

/// (SIM102) Convert `if a: if b:` to `if a and b:`.
pub(crate) fn fix_nested_if_statements(
    locator: &Locator,
    stylist: &Stylist,
    stmt: &rustpython_parser::ast::Stmt,
) -> Result<Edit> {
    // Infer the indentation of the outer block.
    let Some(outer_indent) = whitespace::indentation(locator, stmt) else {
        bail!("Unable to fix multiline statement");
    };

    // Extract the module text.
    let contents = locator.lines(stmt.range());

    // Handle `elif` blocks differently; detect them upfront.
    let is_elif = contents.trim_start().starts_with("elif");

    // If this is an `elif`, we have to remove the `elif` keyword for now. (We'll
    // restore the `el` later on.)
    let module_text = if is_elif {
        Cow::Owned(contents.replacen("elif", "if", 1))
    } else {
        Cow::Borrowed(contents)
    };

    // If the block is indented, "embed" it in a function definition, to preserve
    // indentation while retaining valid source code. (We'll strip the prefix later
    // on.)
    let module_text = if outer_indent.is_empty() {
        module_text
    } else {
        Cow::Owned(format!(
            "def f():{}{module_text}",
            stylist.line_ending().as_str()
        ))
    };

    // Parse the CST.
    let mut tree = match_statement(&module_text)?;

    let statement = if outer_indent.is_empty() {
        &mut tree
    } else {
        let embedding = match_function_def(&mut tree)?;

        let indented_block = match_indented_block(&mut embedding.body)?;
        indented_block.indent = Some(outer_indent);

        let Some(statement) = indented_block.body.first_mut() else {
            bail!("Expected indented block to have at least one statement")
        };
        statement
    };

    let outer_if = match_if(statement)?;

    let If {
        body: Suite::IndentedBlock(ref mut outer_body),
        orelse: None,
        ..
    } = outer_if else {
        bail!("Expected outer if to have indented body and no else")
    };

    let [Statement::Compound(CompoundStatement::If(inner_if @ If { orelse: None, .. }))] =
        &mut *outer_body.body
    else {
        bail!("Expected one inner if statement");
    };

    outer_if.test = Expression::BooleanOperation(Box::new(BooleanOperation {
        left: Box::new(parenthesize_and_operand(outer_if.test.clone())),
        operator: BooleanOp::And {
            whitespace_before: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
            whitespace_after: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
        },
        right: Box::new(parenthesize_and_operand(inner_if.test.clone())),
        lpar: vec![],
        rpar: vec![],
    }));
    outer_if.body = inner_if.body.clone();

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..Default::default()
    };
    tree.codegen(&mut state);

    // Reconstruct and reformat the code.
    let module_text = state.to_string();
    let module_text = if outer_indent.is_empty() {
        &module_text
    } else {
        module_text
            .strip_prefix(&format!("def f():{}", stylist.line_ending().as_str()))
            .unwrap()
    };
    let contents = if is_elif {
        Cow::Owned(module_text.replacen("if", "elif", 1))
    } else {
        Cow::Borrowed(module_text)
    };

    let range = locator.lines_range(stmt.range());
    Ok(Edit::range_replacement(contents.to_string(), range))
}
