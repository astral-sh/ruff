use rustc_hash::FxHashMap;

use ruff_index::{IndexVec, newtype_index};

use crate::frozen::{FrozenMap, NodeTable};
use crate::scope::FileScopeId;

pub use node_key::ExpressionNodeKey;

/// Maps AST nodes to the use IDs assigned within their scopes.
///
/// Nodes in string annotations have a separate, sparse index space.
#[derive(Debug, get_size2::GetSize)]
pub(crate) struct AstIds {
    module: NodeTable<ScopedUseId>,
    annotations: FrozenMap<ExpressionNodeKey, ScopedUseId>,
}

impl AstIds {
    pub(super) fn try_use_id(&self, key: impl Into<ExpressionNodeKey>) -> Option<ScopedUseId> {
        let key = key.into();
        let Some(index) = key.module_index() else {
            return self.annotations.get(&key).copied();
        };
        self.module.get(index).copied()
    }
}

/// Uniquely identifies a use of a name in a [`crate::FileScopeId`].
#[newtype_index]
#[derive(Ord, PartialOrd, get_size2::GetSize)]
pub struct ScopedUseId;

#[derive(Debug, Default)]
pub(super) struct AstIdsBuilder {
    counts: IndexVec<FileScopeId, u32>,
    module: Vec<Option<(FileScopeId, ScopedUseId)>>,
    annotations: FxHashMap<ExpressionNodeKey, (FileScopeId, ScopedUseId)>,
}

impl AstIdsBuilder {
    pub(super) fn push_scope(&mut self) -> FileScopeId {
        self.counts.push(0)
    }

    pub(super) fn record_use(
        &mut self,
        scope: FileScopeId,
        key: impl Into<ExpressionNodeKey>,
    ) -> ScopedUseId {
        let key = key.into();
        let count = &mut self.counts[scope];
        let id = ScopedUseId::from_u32(*count);
        let previous = if let Some(index) = key.module_index() {
            if index >= self.module.len() {
                self.module.resize(index + 1, None);
            }
            self.module[index].replace((scope, id))
        } else {
            self.annotations.insert(key, (scope, id))
        };
        debug_assert!(
            previous.is_none_or(|(owner, _)| owner == scope),
            "a node can only be a use in one scope"
        );
        *count += u32::from(previous.is_none());
        id
    }

    pub(super) fn try_use_id(
        &self,
        scope: FileScopeId,
        key: impl Into<ExpressionNodeKey>,
    ) -> Option<ScopedUseId> {
        let key = key.into();
        let (owner, id) = if let Some(index) = key.module_index() {
            self.module.get(index).copied().flatten()?
        } else {
            *self.annotations.get(&key)?
        };
        (owner == scope).then_some(id)
    }

    pub(super) fn finish(self) -> AstIds {
        let count: usize = self.counts.iter().map(|&count| count as usize).sum();
        AstIds {
            module: NodeTable::from_rows(
                &self.module,
                count - self.annotations.len(),
                |(_, id)| *id,
            ),
            annotations: FrozenMap::from_entries(
                self.annotations
                    .into_iter()
                    .map(|(key, (_, id))| (key, id))
                    .collect(),
            ),
        }
    }
}

/// Node key that can only be constructed for expressions.
pub(crate) mod node_key {
    use ruff_python_ast::{self as ast, flat};

    use crate::{ast_node_ref::AstNodeRef, node_key::NodeKey};

    #[derive(
        Copy,
        Clone,
        Eq,
        PartialEq,
        Ord,
        PartialOrd,
        Hash,
        Debug,
        get_size2::GetSize,
        salsa::SalsaValue,
    )]
    pub struct ExpressionNodeKey(NodeKey);

    impl ExpressionNodeKey {
        /// Only indices in the outer module can address a dense side table.
        pub(super) fn module_index(self) -> Option<usize> {
            self.0.module_index()
        }
    }

    impl From<flat::View<'_, ast::Expr>> for ExpressionNodeKey {
        fn from(value: flat::View<'_, ast::Expr>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<flat::View<'_, ast::ExprName>> for ExpressionNodeKey {
        fn from(value: flat::View<'_, ast::ExprName>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<flat::View<'_, ast::ExprCall>> for ExpressionNodeKey {
        fn from(value: flat::View<'_, ast::ExprCall>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<flat::View<'_, ast::ExprLambda>> for ExpressionNodeKey {
        fn from(value: flat::View<'_, ast::ExprLambda>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<flat::View<'_, ast::Identifier>> for ExpressionNodeKey {
        fn from(value: flat::View<'_, ast::Identifier>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl From<flat::View<'_, ast::Keyword>> for ExpressionNodeKey {
        fn from(value: flat::View<'_, ast::Keyword>) -> Self {
            Self(NodeKey::from_view(value))
        }
    }

    impl<T> From<&AstNodeRef<T>> for ExpressionNodeKey {
        fn from(value: &AstNodeRef<T>) -> Self {
            Self(NodeKey::from_node_ref(value))
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::parsed::parsed_string_annotation;
    use ruff_python_parser::{ParseError, parse_module};

    use super::AstIdsBuilder;

    #[test]
    fn uses_across_blocks_scopes_and_annotations() -> Result<(), ParseError> {
        let source = format!("({}a, b, c, 'T')", "x, ".repeat(63));
        let mut module = parse_module(&source)?;
        module.index_module();
        let [stmt] = module.suite().as_array().unwrap();
        let expr = stmt.as_expr_stmt().unwrap().value();
        let tuple = expr.as_tuple_expr().unwrap();
        let [a, b, c, string] = tuple.elts().slice(63..).as_array().unwrap();
        assert_eq!(a.node_index().as_u32(), Some(63));
        assert_eq!(b.node_index().as_u32(), Some(64));

        let string = string
            .as_string_literal_expr()
            .unwrap()
            .as_single_part_string()
            .unwrap();
        let annotation = parsed_string_annotation(&source, string)?;
        let t = annotation.expr();

        let mut builder = AstIdsBuilder::default();
        let outer = builder.push_scope();
        let inner = builder.push_scope();
        let c_id = builder.record_use(outer, c);
        let a_id = builder.record_use(outer, a);
        let b_id = builder.record_use(inner, b);
        let t_id = builder.record_use(inner, t);
        assert_eq!(builder.try_use_id(outer, b), None);
        assert_eq!(builder.try_use_id(inner, t), Some(t_id));

        let ids = builder.finish();
        assert_eq!(ids.try_use_id(expr), None);
        assert_eq!(ids.try_use_id(a), Some(a_id));
        assert_eq!(ids.try_use_id(b), Some(b_id));
        assert_eq!(ids.try_use_id(c), Some(c_id));
        assert_eq!(ids.try_use_id(t), Some(t_id));
        Ok(())
    }
}
