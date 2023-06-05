use crate::context::NodeLevel;
use crate::prelude::*;
use ruff_formatter::{format_args, write};
use rustpython_parser::ast::Expr;

/// Formats the passed expression. Adds parentheses if the expression doesn't fit on a line.
pub(crate) const fn maybe_parenthesize(expression: &Expr) -> MaybeParenthesize {
    MaybeParenthesize { expression }
}

pub(crate) struct MaybeParenthesize<'a> {
    expression: &'a Expr,
}

impl Format<PyFormatContext<'_>> for MaybeParenthesize<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let saved_level = f.context().node_level();
        f.context_mut().set_node_level(NodeLevel::Parenthesized);

        let result = if needs_parentheses(self.expression) {
            write!(
                f,
                [group(&format_args![
                    if_group_breaks(&text("(")),
                    soft_block_indent(&self.expression.format()),
                    if_group_breaks(&text(")"))
                ])]
            )
        } else {
            // Don't add parentheses around expressions that  have parentheses on their own (e.g. list, dict, tuple, call expression)
            self.expression.format().fmt(f)
        };

        f.context_mut().set_node_level(saved_level);

        result
    }
}

const fn needs_parentheses(expr: &Expr) -> bool {
    !matches!(
        expr,
        Expr::Tuple(_)
            | Expr::List(_)
            | Expr::Set(_)
            | Expr::Dict(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::GeneratorExp(_)
            | Expr::Call(_)
    )
}
