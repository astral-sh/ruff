use ruff_python_ast::{Ranged, StmtClassDef};
use ruff_text_size::TextRange;

use ruff_formatter::write;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};

use crate::comments::trailing_comments;
use crate::expression::parentheses::{parenthesized, Parentheses};
use crate::prelude::*;

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
            type_params: _,
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
            parenthesized(
                "(",
                &FormatInheritanceClause {
                    class_definition: item,
                },
                ")",
            )
            .fmt(f)?;
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
            body,
            ..
        } = self.class_definition;

        let source = f.context().source();

        let mut joiner = f.join_comma_separated(body.first().unwrap().start());

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
                .take_while(|token| token.kind() == SimpleTokenKind::LParen)
                .count();

            // Ignore the first parentheses count
            let parentheses = if left_paren_count > 1 {
                Parentheses::Always
            } else {
                Parentheses::Never
            };

            joiner.entry(first, &first.format().with_options(parentheses));
            joiner.nodes(rest.iter());
        }

        joiner.nodes(keywords.iter()).finish()
    }
}
