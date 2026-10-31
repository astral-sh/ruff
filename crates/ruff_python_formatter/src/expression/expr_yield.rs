use ruff_formatter::write;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;
use ruff_python_ast::{Expr, ExprYield, ExprYieldFrom};
use ruff_text_size::{Ranged, TextRange};

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parenthesize, is_type_annotation_of,
};
use crate::prelude::*;

pub(super) enum AnyExpressionYield<'a> {
    Yield(flat::View<'a, ExprYield>),
    YieldFrom(flat::View<'a, ExprYieldFrom>),
}

impl AnyExpressionYield<'_> {
    const fn is_yield_from(&self) -> bool {
        matches!(self, AnyExpressionYield::YieldFrom(_))
    }

    fn value(&self) -> Option<flat::View<'_, Expr>> {
        match self {
            AnyExpressionYield::Yield(yld) => yld.value(),
            AnyExpressionYield::YieldFrom(yld) => Some(yld.value()),
        }
    }
}

impl Ranged for AnyExpressionYield<'_> {
    fn range(&self) -> TextRange {
        match self {
            AnyExpressionYield::Yield(yld) => yld.range(),
            AnyExpressionYield::YieldFrom(yld) => yld.range(),
        }
    }
}

impl NeedsParentheses for AnyExpressionYield<'_> {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if is_type_annotation_of(self.range(), parent) {
            return OptionalParentheses::Always;
        }

        // According to https://docs.python.org/3/reference/grammar.html There are two situations
        // where we do not want to always parenthesize a yield expression:
        //  1. Right hand side of an assignment, e.g. `x = yield y`
        //  2. Yield statement, e.g. `def foo(): yield y`
        // We catch situation 1 below. Situation 2 does not need to be handled here as
        // FormatStmtExpr, does not add parenthesis
        if parent.is_stmt_assign() || parent.is_stmt_ann_assign() || parent.is_stmt_aug_assign() {
            if let Some(value) = self.value() {
                if context.is_expression_parenthesized(value) {
                    // Ex) `x = yield (1)`
                    OptionalParentheses::Never
                } else {
                    // Ex) `x = yield f(1, 2, 3)`
                    match value.needs_parentheses(self.into(), context) {
                        OptionalParentheses::BestFit => OptionalParentheses::Never,
                        parentheses => parentheses,
                    }
                }
            } else {
                // Ex) `x = yield`
                OptionalParentheses::Never
            }
        } else {
            // Ex) `print((yield))`
            OptionalParentheses::Always
        }
    }
}

impl<'a> From<flat::View<'a, ExprYield>> for AnyExpressionYield<'a> {
    fn from(value: flat::View<'a, ExprYield>) -> Self {
        AnyExpressionYield::Yield(value)
    }
}

impl<'a> From<flat::View<'a, ExprYieldFrom>> for AnyExpressionYield<'a> {
    fn from(value: flat::View<'a, ExprYieldFrom>) -> Self {
        AnyExpressionYield::YieldFrom(value)
    }
}

impl<'a> From<&AnyExpressionYield<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyExpressionYield<'a>) -> Self {
        match value {
            AnyExpressionYield::Yield(yld) => yld.erase(),
            AnyExpressionYield::YieldFrom(yld) => yld.erase(),
        }
    }
}

impl Format<PyFormatContext<'_>> for AnyExpressionYield<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let keyword = if self.is_yield_from() {
            "yield from"
        } else {
            "yield"
        };

        if let Some(val) = self.value() {
            write!(
                f,
                [
                    token(keyword),
                    space(),
                    maybe_parenthesize_expression(val, self, Parenthesize::Optional)
                ]
            )?;
        } else {
            // ExprYieldFrom always has Some(value) so we should never get a bare `yield from`
            write!(f, [token(keyword)])?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct FormatExprYield;

impl FormatNodeRule<ExprYield> for FormatExprYield {
    fn fmt_fields(&self, item: flat::View<'_, ExprYield>, f: &mut PyFormatter) -> FormatResult<()> {
        AnyExpressionYield::from(item).fmt(f)
    }
}

impl NeedsParentheses for flat::View<'_, ExprYield> {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        AnyExpressionYield::from(*self).needs_parentheses(parent, context)
    }
}
