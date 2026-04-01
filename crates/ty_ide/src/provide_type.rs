use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::covering_node;
use ruff_text_size::TextRange;
use ty_python_semantic::{DisplaySettings, HasType, SemanticModel};

pub fn provide_types<I>(db: &dyn Db, file: File, ranges: I) -> Vec<Option<String>>
where
    I: IntoIterator<Item = Option<TextRange>>,
{
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);

    ranges
        .into_iter()
        .map(|range| {
            let range = range?;
            let covering_node = covering_node(parsed.syntax().into(), range);
            let node = match covering_node.find_first(|node| node.is_expression()) {
                Ok(found) => found.node(),
                Err(_) => return None,
            };
            let ty = node.as_expr_ref()?.inferred_type(&model)?;

            Some(
                ty.display_with(db, DisplaySettings::default().fully_qualified())
                    .to_string(),
            )
        })
        .collect()
}
