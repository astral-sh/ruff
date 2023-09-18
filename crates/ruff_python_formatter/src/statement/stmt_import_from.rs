use ruff_formatter::write;
use ruff_python_ast::node::AstNode;
use ruff_python_ast::StmtImportFrom;
use ruff_text_size::Ranged;

use crate::builders::{parenthesize_if_expands, PyFormatterExtensions, TrailingComma};
use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::parentheses::parenthesized;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtImportFrom;

impl FormatNodeRule<StmtImportFrom> for FormatStmtImportFrom {
    fn fmt_fields(&self, item: &StmtImportFrom, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtImportFrom {
            module,
            names,
            level,
            range: _,
        } = item;

        let level_str = level
            .map(|level| ".".repeat(level.to_usize()))
            .unwrap_or(String::default());

        write!(
            f,
            [
                token("from"),
                space(),
                text(&level_str, None),
                module.as_ref().map(AsFormat::format),
                space(),
                token("import"),
                space(),
            ]
        )?;

        if let [name] = names.as_slice() {
            // star can't be surrounded by parentheses
            if name.name.as_str() == "*" {
                return token("*").fmt(f);
            }
        }

        let names = format_with(|f| {
            f.join_comma_separated(item.end())
                .with_trailing_comma(TrailingComma::OneOrMore)
                .entries(names.iter().map(|name| (name, name.format())))
                .finish()
        });

        // A dangling comment on an import is a parenthesized comment, like:
        // ```python
        // from example import (  # comment
        //     A,
        //     B,
        // )
        // ```
        let comments = f.context().comments().clone();
        let parenthesized_comments = comments.dangling(item.as_any_node_ref());

        if parenthesized_comments.is_empty() {
            parenthesize_if_expands(&names).fmt(f)
        } else {
            parenthesized("(", &names, ")")
                .with_dangling_comments(parenthesized_comments)
                .fmt(f)
        }
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
