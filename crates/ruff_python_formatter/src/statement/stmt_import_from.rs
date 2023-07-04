use crate::builders::{optional_parentheses, PyFormatterExtensions};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{dynamic_text, format_with, space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use rustpython_parser::ast::StmtImportFrom;

#[derive(Default)]
pub struct FormatStmtImportFrom;

impl FormatNodeRule<StmtImportFrom> for FormatStmtImportFrom {
    fn fmt_fields(&self, item: &StmtImportFrom, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtImportFrom {
            module,
            names,
            range: _,
            level,
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
            f.join_comma_separated()
                .entries(names.iter().map(|name| (name, name.format())))
                .finish()
        });
        optional_parentheses(&names).fmt(f)
    }
}
