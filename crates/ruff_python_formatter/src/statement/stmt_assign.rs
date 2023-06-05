use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, Parentheses, Parenthesize};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::formatter::Formatter;
use ruff_formatter::prelude::{group, if_group_breaks, soft_block_indent, space, text};
use ruff_formatter::{format_args, write, Buffer, Format, FormatResult};
use ruff_python_ast::prelude::{Expr, Ranged};
use ruff_python_ast::token_kind::TokenKind::False;
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::StmtAssign;

//
// Note: This currently does wrap but not the black way so the types below likely need to be
// replaced entirely
//

#[derive(Default)]
pub struct FormatStmtAssign;

impl FormatNodeRule<StmtAssign> for FormatStmtAssign {
    fn fmt_fields(&self, item: &StmtAssign, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssign {
            range: _,
            targets,
            value,
            type_comment: _,
        } = item;
        write!(
            f,
            [
                LhsAssignList::new(targets),
                value.format().with_options(Parenthesize::IfBreaks)
            ]
        )
    }
}

#[derive(Debug)]
struct LhsAssignList<'a> {
    lhs_assign_list: &'a [Expr],
}

impl<'a> LhsAssignList<'a> {
    const fn new(lhs_assign_list: &'a [Expr]) -> Self {
        Self { lhs_assign_list }
    }
}

fn is_parenthesized(
    range: TextRange,
    elts: &[Expr],
    f: &mut Formatter<PyFormatContext<'_>>,
) -> bool {
    let parentheses = "(";
    let first_char = &f.context().contents()[TextRange::at(range.start(), parentheses.text_len())];
    if first_char != parentheses {
        return false;
    }

    // Consider `a = (1, 2), 3`: The first char of the current expr starts is a parentheses, but
    // it's not its own but that of its first tuple child. We know that it belongs to the child
    // because if it wouldn't, the child would start (at least) a char later
    let Some(first_child) = elts.first() else {
        return false;
    };
    first_child.range().start() != range.start()
}

fn expr_children(expr: &Expr) -> &[Expr] {
    match expr {
        Expr::BoolOp(_) => todo!(),
        Expr::NamedExpr(_) => todo!(),
        Expr::BinOp(_) => todo!(),
        Expr::UnaryOp(_) => todo!(),
        Expr::Lambda(_) => todo!(),
        Expr::IfExp(_) => todo!(),
        Expr::Dict(_) => todo!(),
        Expr::Set(_) => todo!(),
        Expr::ListComp(_) => todo!(),
        Expr::SetComp(_) => todo!(),
        Expr::DictComp(_) => todo!(),
        Expr::GeneratorExp(_) => todo!(),
        Expr::Await(_) => todo!(),
        Expr::Yield(_) => todo!(),
        Expr::YieldFrom(_) => todo!(),
        Expr::Compare(_) => todo!(),
        Expr::Call(_) => todo!(),
        Expr::FormattedValue(_) => todo!(),
        Expr::JoinedStr(_) => todo!(),
        Expr::Constant(_) => todo!(),
        Expr::Attribute(_) => todo!(),
        Expr::Subscript(_) => todo!(),
        Expr::Starred(_) => todo!(),
        Expr::Name(_) => todo!(),
        Expr::List(_) => todo!(),
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::Slice(_) => todo!(),
    }
}

impl Format<PyFormatContext<'_>> for LhsAssignList<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        for element in self.lhs_assign_list {
            let parentheses =
                element.needs_parentheses(Parenthesize::Optional, f.context().contents());

            if parentheses == Parentheses::Never && false {
                write!(
                    f,
                    [
                        group(&format_args![soft_block_indent(
                            &element.format().with_options(Parenthesize::IfBreaks)
                        ),]),
                        space(),
                        text("="),
                        space(),
                    ]
                )?;
            } else {
                write!(
                    f,
                    [
                        group(&format_args![
                            if_group_breaks(&text("(")),
                            soft_block_indent(
                                &element.format().with_options(Parenthesize::Optional)
                            ),
                            if_group_breaks(&text(")"))
                        ]),
                        space(),
                        text("="),
                        space(),
                    ]
                )?;
            }
        }
        Ok(())
    }
}
