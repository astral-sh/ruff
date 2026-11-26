use rustc_hash::FxHashMap;
use ty_python_core::ExpressionNodeKey;
use ty_python_core::frozen::FrozenMap;

use crate::types::Type;

/// Expression types collected while inferring a region.
///
/// Inference usually visits nodes close to their AST order. Keeping those entries sorted
/// lets us append whole child regions and freeze the result without sorting it again.
#[derive(Default, Debug)]
pub(super) struct ExpressionMap<'db> {
    sorted: Vec<(ExpressionNodeKey, Type<'db>)>,
    /// When present, holds all entries and leaves `sorted` empty. Boxing it keeps
    /// the builder small when the vector alone suffices.
    spill: Option<Box<FxHashMap<ExpressionNodeKey, Type<'db>>>>,
    /// Total number of entries shifted by out-of-order insertions.
    moved: usize,
}

impl<'db> ExpressionMap<'db> {
    /// Record a type and return any previous type for the same node.
    ///
    /// If inserting into the sorted vector would exceed the cumulative shift budget,
    /// switch to a hash map to avoid quadratic work on widely reordered nodes.
    #[inline]
    pub(super) fn insert(&mut self, key: ExpressionNodeKey, value: Type<'db>) -> Option<Type<'db>> {
        // In a list like `[1, -2]`, we record `2` before `-2`. Checking the last
        // two keys lets us insert `-2` between `1` and `2` without a binary search.
        const TAIL_SEARCH: usize = 2;
        const SHIFTS_PER_ENTRY: usize = 32;

        if let Some(map) = &mut self.spill {
            return map.insert(key, value);
        }
        if self.sorted.last().is_none_or(|&(last, _)| last < key) {
            self.sorted.push((key, value));
            return None;
        }

        let mut pos = self.sorted.len();
        let position = loop {
            if pos == 0 {
                break Err(0);
            }
            if self.sorted.len() - pos == TAIL_SEARCH {
                break self.sorted[..pos].binary_search_by_key(&key, |&(k, _)| k);
            }
            match self.sorted[pos - 1].0.cmp(&key) {
                std::cmp::Ordering::Equal => break Ok(pos - 1),
                std::cmp::Ordering::Less => break Err(pos),
                std::cmp::Ordering::Greater => pos -= 1,
            }
        };
        match position {
            Ok(i) => Some(std::mem::replace(&mut self.sorted[i].1, value)),
            Err(i) => {
                self.moved += self.sorted.len() - i;
                if self.moved > SHIFTS_PER_ENTRY * self.sorted.len() {
                    let mut map: FxHashMap<_, _> =
                        std::mem::take(&mut self.sorted).into_iter().collect();
                    map.insert(key, value);
                    self.spill = Some(Box::new(map));
                } else {
                    self.sorted.insert(i, (key, value));
                }
                None
            }
        }
    }

    pub(super) fn get(&self, key: ExpressionNodeKey) -> Option<&Type<'db>> {
        if let Some(map) = &self.spill {
            return map.get(&key);
        }
        self.sorted
            .binary_search_by_key(&key, |&(k, _)| k)
            .ok()
            .map(|i| &self.sorted[i].1)
    }

    pub(super) fn len(&self) -> usize {
        self.spill
            .as_ref()
            .map_or(self.sorted.len(), |map| map.len())
    }

    /// Visit every node once, in unspecified order until the map is frozen.
    pub(super) fn iter(&self) -> impl Iterator<Item = (ExpressionNodeKey, Type<'db>)> + '_ {
        self.sorted.iter().copied().chain(
            self.spill
                .iter()
                .flat_map(|map| map.iter().map(|(k, v)| (*k, *v))),
        )
    }

    pub(super) fn extend(
        &mut self,
        values: impl IntoIterator<Item = (ExpressionNodeKey, Type<'db>)>,
    ) {
        for (key, value) in values {
            self.insert(key, value);
        }
    }

    /// Merge entries with unique keys in ascending order, appending them directly
    /// when they follow all existing keys in the sorted vector.
    pub(super) fn extend_sorted(&mut self, entries: &[(ExpressionNodeKey, Type<'db>)]) {
        let Some(&(first, _)) = entries.first() else {
            return;
        };
        if self.spill.is_none() && self.sorted.last().is_none_or(|&(last, _)| last < first) {
            self.sorted.extend_from_slice(entries);
        } else {
            self.extend(entries.iter().copied());
        }
    }

    /// Return entries in node order, sorting only if we switched to a hash map.
    pub(super) fn into_sorted(self) -> Vec<(ExpressionNodeKey, Type<'db>)> {
        if let Some(map) = self.spill {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_unstable_by_key(|(key, _)| *key);
            entries
        } else {
            self.sorted
        }
    }

    pub(super) fn freeze(self) -> FrozenMap<ExpressionNodeKey, Type<'db>> {
        FrozenMap::from_sorted(self.into_sorted())
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_python_ast as ast;
    use ruff_text_size::TextRange;
    use ty_python_core::frozen::FrozenValueMap;
    use ty_python_core::{
        ExpressionNodeKey, ProgramFile, Truthiness, global_scope, semantic_index,
    };

    use super::ExpressionMap;
    use crate::db::tests::setup_db;
    use crate::types::infer::{InferenceRegion, TypeInferenceBuilder};
    use crate::types::{Type, TypeContext};

    fn key(index: u32) -> ExpressionNodeKey {
        let name = ast::Identifier::new("x", TextRange::default());
        name.node_index.set(index.into());
        (&name).into()
    }

    #[test]
    fn replacements_in_the_tail_and_prefix() {
        let mut map = ExpressionMap::default();
        map.extend((1..9).map(|i| (key(i), Type::Never)));
        assert_eq!(map.insert(key(0), Type::Never), None);
        for index in [8, 0] {
            assert_eq!(map.insert(key(index), Type::unknown()), Some(Type::Never));
            assert_eq!(map.get(key(index)), Some(&Type::unknown()));
        }
        assert!(map.spill.is_none());
        assert!(map.freeze().iter().map(|(k, _)| *k).eq((0..9).map(key)));
    }

    #[test]
    fn reversed_keys_switch_to_hashing() {
        let mut map = ExpressionMap::default();
        map.extend((0..128).rev().map(|i| (key(i), Type::Never)));
        assert!(map.spill.is_some());
        assert_eq!(map.insert(key(100), Type::unknown()), Some(Type::Never));
        assert_eq!(map.get(key(100)), Some(&Type::unknown()));
        assert_eq!(map.iter().count(), 128);
        let frozen = map.freeze();
        assert!(frozen.iter().map(|(k, _)| *k).eq((0..128).map(key)));
        assert_eq!(frozen.get(&key(100)), Some(&Type::unknown()));
    }

    #[test]
    fn scope_value_indices_are_independent_of_insertion_order() {
        let entries = (0..128).map(|i| (key(i), Type::int_literal(i64::from(i % 3))));
        let mut sorted = ExpressionMap::default();
        sorted.extend(entries.clone());
        let mut reversed = ExpressionMap::default();
        reversed.extend(entries.rev());
        assert!(reversed.spill.is_some());

        assert_eq!(
            FrozenValueMap::from_sorted(sorted.into_sorted()),
            FrozenValueMap::from_sorted(reversed.into_sorted())
        );
    }

    #[test]
    fn merging_sorted_regions_replaces_types_and_clears_stale_truthiness() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_file("/src/main.py", "")?;
        let file = system_path_to_file(&db, "/src/main.py")?;
        let env = db.program_environment();
        let program_file = ProgramFile::new(&db, file, env.program(&db));
        let region =
            InferenceRegion::Scope(global_scope(&db, program_file), TypeContext::default());
        let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
        let mut builder = TypeInferenceBuilder::new(
            &db,
            &env,
            region,
            file,
            program_file,
            semantic_index(&db, program_file),
            &module,
        );

        builder.extend_sorted(&[]);
        builder.extend_sorted(&[(key(0), Type::Never)]);
        builder.extend_sorted(&[(key(2), Type::Never), (key(3), Type::Never)]);
        builder.extend_sorted(&[(key(1), Type::Never), (key(2), Type::unknown())]);
        builder
            .comparison_truthiness
            .insert(key(2), Truthiness::AlwaysTrue);
        builder
            .comparison_truthiness
            .insert(key(3), Truthiness::AlwaysFalse);
        builder.extend_sorted(&[(key(3), Type::unknown())]);

        let _ = builder.context.finish();
        assert!(builder.comparison_truthiness.contains_key(&key(2)));
        assert!(!builder.comparison_truthiness.contains_key(&key(3)));
        assert_eq!(
            builder.expressions.freeze().iter().as_slice(),
            &[
                (key(0), Type::Never),
                (key(1), Type::Never),
                (key(2), Type::unknown()),
                (key(3), Type::unknown()),
            ]
        );
        Ok(())
    }
}
