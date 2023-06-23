use crate::comments::trailing_comments;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::trivia::{SimpleTokenizer, TokenKind};
use ruff_formatter::{format_args, write};
use ruff_text_size::TextRange;
use rustpython_parser::ast::{Ranged, StmtClassDef};

#[derive(Default)]
pub struct FormatStmtClassDef;

impl FormatNodeRule<StmtClassDef> for FormatStmtClassDef {
    fn fmt_fields(&self, item: &StmtClassDef, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtClassDef {
            range: _,
            name,
            bases,
            keywords,
            body,
            decorator_list,
        } = item;

        f.join_with(hard_line_break())
            .entries(decorator_list.iter().formatted())
            .finish()?;

        if !decorator_list.is_empty() {
            hard_line_break().fmt(f)?;
        }

        write!(f, [text("class"), space(), name.format()])?;

        if !(bases.is_empty() && keywords.is_empty()) {
            write!(
                f,
                [group(&format_args![
                    text("("),
                    soft_block_indent(&FormatInheritanceClause {
                        class_definition: item
                    }),
                    text(")")
                ])]
            )?;
        }

        let comments = f.context().comments().clone();
        let trailing_head_comments = comments.dangling_comments(item);

        write!(
            f,
            [
                text(":"),
                trailing_comments(trailing_head_comments),
                block_indent(&body.format())
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _node: &StmtClassDef,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // handled in fmt_fields
        Ok(())
    }
}

struct FormatInheritanceClause<'a> {
    class_definition: &'a StmtClassDef,
}

impl Format<PyFormatContext<'_>> for FormatInheritanceClause<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let StmtClassDef {
            bases,
            keywords,
            name,
            ..
        } = self.class_definition;

        let source = f.context().contents();

        let mut joiner = f.join_comma_separated();

        if let Some((first, rest)) = bases.split_first() {
            // Manually handle parentheses for the first expression because the logic in `FormatExpr`
            // doesn't know that it should disregard the parentheses of the inheritance clause.
            // ```python
            // class Test(A) # A is not parenthesized, the parentheses belong to the inheritance clause
            // class Test((A)) # A is parenthesized
            // ```
            // parentheses from the inheritance clause belong to the expression.
            let tokenizer = SimpleTokenizer::new(source, TextRange::new(name.end(), first.start()))
                .skip_trivia();

            let left_paren_count = tokenizer
                .take_while(|token| token.kind() == TokenKind::LParen)
                .count();

            // Ignore the first parentheses count
            let parenthesize = if left_paren_count > 1 {
                Parenthesize::Always
            } else {
                Parenthesize::Never
            };

            joiner.entry(first, &first.format().with_options(parenthesize));
            joiner.nodes(rest.iter());
        }

        joiner.nodes(keywords.iter()).finish()
    }
}
