use ruff_formatter::write;
use ruff_python_ast::AstNode;
use ruff_python_ast::StmtImportFrom;
use ruff_text_size::Ranged;

use crate::builders::{parenthesize_if_expands, PyFormatterExtensions, TrailingComma};
use crate::comments::SourceComment;
use crate::expression::parentheses::parenthesized;
use crate::has_skip_comment;
use crate::other::identifier::DotDelimitedIdentifier;
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

        write!(
            f,
            [
                token("from"),
                space(),
                format_with(|f| {
                    for _ in 0..*level {
                        token(".").fmt(f)?;
                    }
                    Ok(())
                }),
                module.as_ref().map(DotDelimitedIdentifier::new),
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

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        has_skip_comment(trailing_comments, context.source())
    }
}
