use ruff_python_ast as ast;

use crate::types::{Type, UnionBuilder};
use crate::{Db, ProgramEnvironment};

/// Recover the key and value types of an immediately consumed dictionary literal, without
/// promoting its elements. The dictionary and its elements must already have been inferred:
/// their ordinary types still describe mutable containers, including any nested containers.
///
/// These unions conservatively include overwritten entries. They do not describe the dictionary's
/// length, iteration order, or the relationship between individual keys and values.
pub(super) fn dict_literal_key_value_types<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    dict: &ast::ExprDict,
    mut expression_type: impl FnMut(&ast::Expr) -> Type<'db>,
) -> Option<(Type<'db>, Type<'db>)> {
    fn extend<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        dict: &ast::ExprDict,
        expression_type: &mut impl FnMut(&ast::Expr) -> Type<'db>,
        keys: &mut UnionBuilder<'db>,
        values: &mut UnionBuilder<'db>,
    ) -> Option<()> {
        for item in &dict.items {
            let (key, value) = if let Some(key) = &item.key {
                (expression_type(key), expression_type(&item.value))
            } else if let ast::Expr::Dict(unpacked) = &item.value {
                extend(db, env, unpacked, expression_type, keys, values)?;
                continue;
            } else {
                expression_type(&item.value).unpack_keys_and_items(db, env)?
            };
            keys.add_in_place(key);
            values.add_in_place(value);
        }
        Some(())
    }

    let mut keys = UnionBuilder::new(db, env);
    let mut values = UnionBuilder::new(db, env);
    extend(db, env, dict, &mut expression_type, &mut keys, &mut values)?;
    let keys = keys.build();
    let values = values.build();
    // Keep the ordinary inference fallback for an empty dictionary.
    (!keys.is_never()).then_some((keys, values))
}
