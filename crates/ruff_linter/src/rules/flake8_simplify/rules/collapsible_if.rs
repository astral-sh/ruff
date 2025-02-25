use std::borrow::Cow;

use anyhow::{bail, Result};
use libcst_native::ParenthesizedNode;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{self as ast, whitespace, ElifElseClause, Expr, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::cst::helpers::space;
use crate::cst::matchers::{match_function_def, match_if, match_indented_block, match_statement};
use crate::fix::codemods::CodegenStylist;
use crate::fix::edits::fits;
use crate::Locator;

/// ## What it does
/// Checks for nested `if` statements that can be collapsed into a single `if`
/// statement.
///
/// ## Why is this bad?
/// Nesting `if` statements leads to deeper indentation and makes code harder to
/// read. Instead, combine the conditions into a single `if` statement with an
/// `and` operator.
///
/// ## Example
/// ```python
/// if foo:
///     if bar:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// if foo and bar:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: The `if` statement](https://docs.python.org/3/reference/compound_stmts.html#the-if-statement)
/// - [Python documentation: Boolean operations](https://docs.python.org/3/reference/expressions.html#boolean-operations)
#[derive(ViolationMetadata)]
pub(crate) struct CollapsibleIf;

impl Violation for CollapsibleIf {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use a single `if` statement instead of nested `if` statements".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Combine `if` statements using `and`".to_string())
    }
}

/// SIM102
pub(crate) fn nested_if_statements(
    checker: &Checker,
    stmt_if: &ast::StmtIf,
    parent: Option<&Stmt>,
) {
    let Some(nested_if) = nested_if_body(stmt_if) else {
        return;
    };

    // Find the deepest nested if-statement, to inform the range.
    let Some(test) = find_last_nested_if(nested_if.body()) else {
        return;
    };

    // Check if the parent is already emitting a larger diagnostic including this if statement
    if let Some(Stmt::If(stmt_if)) = parent {
        if let Some(nested_if) = nested_if_body(stmt_if) {
            // In addition to repeating the `nested_if_body` and `find_last_nested_if` check, we
            // also need to be the first child in the parent
            let body = nested_if.body();
            if matches!(&body[0], Stmt::If(inner) if *inner == *stmt_if)
                && find_last_nested_if(body).is_some()
            {
                return;
            }
        }
    }

    let Some(colon) = SimpleTokenizer::starts_at(test.end(), checker.locator().contents())
        .skip_trivia()
        .find(|token| token.kind == SimpleTokenKind::Colon)
    else {
        return;
    };

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if is_sys_version_block(stmt_if, checker.semantic()) {
        return;
    }

    // Avoid suggesting ternary for `if TYPE_CHECKING:`-style checks.
    if is_type_checking_block(stmt_if, checker.semantic()) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        CollapsibleIf,
        TextRange::new(nested_if.start(), colon.end()),
    );
    // The fixer preserves comments in the nested body, but removes comments between
    // the outer and inner if statements.
    if !checker.comment_ranges().intersects(TextRange::new(
        nested_if.start(),
        nested_if.body()[0].start(),
    )) {
        diagnostic.try_set_optional_fix(|| {
            match collapse_nested_if(checker.locator(), checker.stylist(), nested_if) {
                Ok(edit) => {
                    if edit.content().is_none_or(|content| {
                        fits(
                            content,
                            (&nested_if).into(),
                            checker.locator(),
                            checker.settings.pycodestyle.max_line_length,
                            checker.settings.tab_size,
                        )
                    }) {
                        Ok(Some(Fix::unsafe_edit(edit)))
                    } else {
                        Ok(None)
                    }
                }
                Err(err) => bail!("Failed to collapse `if`: {err}"),
            }
        });
    }
    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Clone, Copy)]
pub(super) enum NestedIf<'a> {
    If(&'a ast::StmtIf),
    Elif(&'a ElifElseClause),
}

impl<'a> NestedIf<'a> {
    pub(super) fn body(self) -> &'a [Stmt] {
        match self {
            NestedIf::If(stmt_if) => &stmt_if.body,
            NestedIf::Elif(clause) => &clause.body,
        }
    }

    pub(super) fn is_elif(self) -> bool {
        matches!(self, NestedIf::Elif(..))
    }
}

impl Ranged for NestedIf<'_> {
    fn range(&self) -> TextRange {
        match self {
            NestedIf::If(stmt_if) => stmt_if.range(),
            NestedIf::Elif(clause) => clause.range(),
        }
    }
}

impl<'a> From<&NestedIf<'a>> for AnyNodeRef<'a> {
    fn from(value: &NestedIf<'a>) -> Self {
        match value {
            NestedIf::If(stmt_if) => (*stmt_if).into(),
            NestedIf::Elif(clause) => (*clause).into(),
        }
    }
}

/// Returns the body, the range of the `if` or `elif` and whether the range is for an `if` or `elif`
fn nested_if_body(stmt_if: &ast::StmtIf) -> Option<NestedIf> {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        ..
    } = stmt_if;

    // It must be the last condition, otherwise there could be another `elif` or `else` that only
    // depends on the outer of the two conditions
    let (test, nested_if) = if let Some(clause) = elif_else_clauses.last() {
        if let Some(test) = &clause.test {
            (test, NestedIf::Elif(clause))
        } else {
            // The last condition is an `else` (different rule)
            return None;
        }
    } else {
        (test.as_ref(), NestedIf::If(stmt_if))
    };

    // The nested if must be the only child, otherwise there is at least one more statement that
    // only depends on the outer condition
    if body.len() > 1 {
        return None;
    }

    // Allow `if __name__ == "__main__":` statements.
    if is_main_check(test) {
        return None;
    }

    // Allow `if True:` and `if False:` statements.
    if test.is_boolean_literal_expr() {
        return None;
    }

    Some(nested_if)
}

/// Find the last nested if statement and return the test expression and the
/// last statement.
///
/// ```python
/// if xxx:
///     if yyy:
///      # ^^^ returns this expression
///         z = 1
///         ...
/// ```
fn find_last_nested_if(body: &[Stmt]) -> Option<&Expr> {
    let [Stmt::If(ast::StmtIf {
        test,
        body: inner_body,
        elif_else_clauses,
        ..
    })] = body
    else {
        return None;
    };
    if !elif_else_clauses.is_empty() {
        return None;
    }
    find_last_nested_if(inner_body).or(Some(test))
}

/// Returns `true` if an expression is an `if __name__ == "__main__":` check.
fn is_main_check(expr: &Expr) -> bool {
    if let Expr::Compare(ast::ExprCompare {
        left, comparators, ..
    }) = expr
    {
        if let Expr::Name(ast::ExprName { id, .. }) = left.as_ref() {
            if id == "__name__" {
                if let [Expr::StringLiteral(ast::ExprStringLiteral { value, .. })] = &**comparators
                {
                    if value == "__main__" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn parenthesize_and_operand(expr: libcst_native::Expression) -> libcst_native::Expression {
    match &expr {
        _ if !expr.lpar().is_empty() => expr,
        libcst_native::Expression::BooleanOperation(boolean_operation)
            if matches!(
                boolean_operation.operator,
                libcst_native::BooleanOp::Or { .. }
            ) =>
        {
            expr.with_parens(
                libcst_native::LeftParen::default(),
                libcst_native::RightParen::default(),
            )
        }
        libcst_native::Expression::IfExp(_)
        | libcst_native::Expression::Lambda(_)
        | libcst_native::Expression::NamedExpr(_) => expr.with_parens(
            libcst_native::LeftParen::default(),
            libcst_native::RightParen::default(),
        ),
        _ => expr,
    }
}

/// Convert `if a: if b:` to `if a and b:`.
pub(super) fn collapse_nested_if(
    locator: &Locator,
    stylist: &Stylist,
    nested_if: NestedIf,
) -> Result<Edit> {
    // Infer the indentation of the outer block.
    let Some(outer_indent) = whitespace::indentation(locator.contents(), &nested_if) else {
        bail!("Unable to fix multiline statement");
    };

    // Extract the module text.
    let contents = locator.lines_str(nested_if.range());

    // If this is an `elif`, we have to remove the `elif` keyword for now. (We'll
    // restore the `el` later on.)
    let module_text = if nested_if.is_elif() {
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

    let libcst_native::If {
        body: libcst_native::Suite::IndentedBlock(ref mut outer_body),
        orelse: None,
        ..
    } = outer_if
    else {
        bail!("Expected outer if to have indented body and no else")
    };

    let [libcst_native::Statement::Compound(libcst_native::CompoundStatement::If(
        inner_if @ libcst_native::If { orelse: None, .. },
    ))] = &mut *outer_body.body
    else {
        bail!("Expected one inner if statement");
    };

    outer_if.test =
        libcst_native::Expression::BooleanOperation(Box::new(libcst_native::BooleanOperation {
            left: Box::new(parenthesize_and_operand(outer_if.test.clone())),
            operator: libcst_native::BooleanOp::And {
                whitespace_before: space(),
                whitespace_after: space(),
            },
            right: Box::new(parenthesize_and_operand(inner_if.test.clone())),
            lpar: vec![],
            rpar: vec![],
        }));
    outer_if.body = inner_if.body.clone();

    // Reconstruct and reformat the code.
    let module_text = tree.codegen_stylist(stylist);
    let module_text = if outer_indent.is_empty() {
        &module_text
    } else {
        module_text
            .strip_prefix(&format!("def f():{}", stylist.line_ending().as_str()))
            .unwrap()
    };
    let contents = if nested_if.is_elif() {
        Cow::Owned(module_text.replacen("if", "elif", 1))
    } else {
        Cow::Borrowed(module_text)
    };

    let range = locator.lines_range(nested_if.range());
    Ok(Edit::range_replacement(contents.to_string(), range))
}
