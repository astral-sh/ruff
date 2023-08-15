use ruff_formatter::prelude::{dynamic_text, format_with, space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use ruff_python_ast::node::AstNode;
use ruff_python_ast::{Ranged, StmtImportFrom};

use crate::builders::{parenthesize_if_expands, PyFormatterExtensions};
use crate::expression::parentheses::parenthesized;
use crate::{AsFormat, FormatNodeRule, PyFormatter};

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
                text("from"),
                space(),
                dynamic_text(&level_str, None),
                module.as_ref().map(AsFormat::format),
                space(),
                text("import"),
                space(),
            ]
        )?;

        if let [name] = names.as_slice() {
            // star can't be surrounded by parentheses
            if name.name.as_str() == "*" {
                return text("*").fmt(f);
            }
        }

        let names = format_with(|f| {
            f.join_comma_separated(item.end())
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
        let parenthesized_comments = comments.dangling_comments(item.as_any_node_ref());

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
        _node: &StmtImportFrom,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
