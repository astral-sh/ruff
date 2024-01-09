use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::line_width::LineWidthBuilder;
use crate::settings::LinterSettings;

/// ## What it does
/// Checks for chained operators where adding parentheses could improve the
/// clarity of the code.
///
/// ## Why is this bad?
/// `and` always binds more tightly than `or` when chaining the two together,
/// but this can be hard to remember (and sometimes surprising).
/// Adding parentheses in these situations can greatly improve code readability,
/// with no change to semantics or performance.
///
/// For example:
/// ```python
/// a, b, c = 1, 0, 2
/// x = a or b and c
///
/// d, e, f = 0, 1, 2
/// y = d and e or f
/// ```
///
/// Use instead:
/// ```python
/// a, b, c = 1, 0, 2
/// x = a or (b and c)
///
/// d, e, f = 0, 1, 2
/// y = (d and e) or f
/// ````
#[violation]
pub struct ParenthesizeChainedOperators;

impl Violation for ParenthesizeChainedOperators {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Parenthesize `a and b` expressions when chaining `and` and `or` together, to make the precedence clear"
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some(
            "Put parentheses around the `and` subexpression inside the `or` expression".to_string(),
        )
    }
}

/// RUF021
pub(crate) fn parenthesize_chained_logical_operators(
    checker: &mut Checker,
    expr: &ast::ExprBoolOp,
) {
    // We're only interested in `and` expressions inside `or` expressions:
    // - `a or b or c` => `BoolOp(values=[Name("a"), Name("b"), Name("c")], op=Or)`
    // - `a and b and c` => `BoolOp(values=[Name("a"), Name("b"), Name("c")], op=And)`
    // - `a or b and c` => `BoolOp(value=[Name("a"), BoolOp(values=[Name("b"), Name("c")], op=And), op=Or)`
    //
    // While it is *possible* to get an `Or` node inside an `And` node,
    // you can only achieve it by parenthesizing the `or` subexpression
    // (since normally, `and` always binds more tightly):
    // - `a and (b or c)` => `BoolOp(values=[Name("a"), BoolOp(values=[Name("b"), Name("c"), op=Or), op=And)`
    //
    // We only care about unparenthesized boolean subexpressions here
    // (if they're parenthesized already, that's great!),
    // so we can ignore all cases where an `Or` node
    // exists inside an `And` node.
    if expr.op.is_and() {
        return;
    }
    for condition in &expr.values {
        match condition {
            ast::Expr::BoolOp(
                bool_op @ ast::ExprBoolOp {
                    op: ast::BoolOp::And,
                    ..
                },
            ) => {
                let locator = checker.locator();
                let source_range = bool_op.range();
                if parenthesized_range(
                    bool_op.into(),
                    expr.into(),
                    checker.indexer().comment_ranges(),
                    locator.contents(),
                )
                .is_none()
                {
                    let mut diagnostic =
                        Diagnostic::new(ParenthesizeChainedOperators, source_range);
                    if is_single_line_expr_with_narrow_width(
                        locator,
                        source_range,
                        checker.settings,
                    ) {
                        let new_source = format!("({})", locator.slice(source_range));
                        diagnostic.set_fix(Fix::applicable_edit(
                            Edit::range_replacement(new_source, source_range),
                            Applicability::Safe,
                        ));
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
            _ => continue,
        };
    }
}

fn is_single_line_expr_with_narrow_width(
    locator: &Locator,
    source_range: TextRange,
    settings: &LinterSettings,
) -> bool {
    if locator.contains_line_break(source_range) {
        return false;
    }
    let width_with_fix = LineWidthBuilder::new(settings.tab_size)
        .add_str(locator.full_line(source_range.start()).trim_end())
        .add_str("()");
    width_with_fix <= settings.line_length
}
