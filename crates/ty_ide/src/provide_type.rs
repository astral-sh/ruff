use crate::Db;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::covering_node;
use ruff_python_ast::{AnyNodeRef, ExprRef};
use ruff_text_size::{Ranged, TextRange};
use ty_python_semantic::types::Type;
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
            let ty = match covering_node.find_first(AnyNodeRef::is_expression) {
                Ok(found) => expression_type(&model, found.node())?,
                Err(covering_node) => {
                    let handler = covering_node
                        .find_first(|node| {
                            matches!(node, AnyNodeRef::ExceptHandlerExceptHandler(_))
                        })
                        .ok()?
                        .node();
                    let AnyNodeRef::ExceptHandlerExceptHandler(handler) = handler else {
                        return None;
                    };
                    if !handler
                        .name
                        .as_ref()
                        .is_some_and(|name| name.range().contains_range(range))
                    {
                        return None;
                    }
                    handler.inferred_type(&model)?
                }
            };

            Some(
                ty.display_with(db, DisplaySettings::default().fully_qualified())
                    .to_string(),
            )
        })
        .collect()
}

fn expression_type<'db>(model: &SemanticModel<'db>, node: AnyNodeRef<'_>) -> Option<Type<'db>> {
    let expression = node.as_expr_ref()?;
    let inferred = expression.inferred_type(model)?;

    let ExprRef::Name(name) = expression else {
        return Some(inferred);
    };
    let members = model.members_in_scope_at(node);
    let Some(value_ty) = members.get(&name.id).map(|member| member.ty) else {
        return Some(inferred);
    };

    // Names in annotations are inferred as their instance type, but provide-type reports the
    // runtime value type of the expression.
    if value_ty.is_class_literal() && !inferred.is_class_literal() {
        Some(value_ty)
    } else {
        Some(inferred)
    }
}
