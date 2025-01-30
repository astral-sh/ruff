use crate::types::{infer_scope_types, semantic_index, Type};
use crate::Db;
use ruff_db::files::File;
use rustc_hash::FxHashMap;

/// Get type-coverage statistics for a file.
#[salsa::tracked(return_ref)]
pub fn type_statistics<'db>(db: &'db dyn Db, file: File) -> TypeStatistics<'db> {
    let _span = tracing::trace_span!("type_statistics", file=?file.path(db)).entered();

    tracing::debug!(
        "Gathering statistics for file '{path}'",
        path = file.path(db)
    );

    let index = semantic_index(db, file);
    let mut statistics = TypeStatistics::default();

    for scope_id in index.scope_ids() {
        let result = infer_scope_types(db, scope_id);
        statistics.extend(&result.statistics());
    }

    statistics
}

/// Map each type to count of expressions with that type.
#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct TypeStatistics<'db>(FxHashMap<Type<'db>, u32>);

impl<'db> TypeStatistics<'db> {
    fn extend(&mut self, other: &TypeStatistics<'db>) {
        self.0.extend(&other.0);
    }

    pub(super) fn increment(&mut self, ty: Type<'db>) {
        self.0
            .entry(ty)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }

    #[allow(unused)]
    fn expression_count(&self) -> u32 {
        self.0.values().sum()
    }

    #[allow(unused)]
    fn todo_count(&self) -> u32 {
        self.0
            .iter()
            .filter(|(key, _)| key.is_todo())
            .map(|(_, count)| count)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{setup_db, TestDb};
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithTestSystem;

    fn get_stats<'db>(
        db: &'db mut TestDb,
        filename: &str,
        source: &str,
    ) -> &'db TypeStatistics<'db> {
        db.write_dedented(filename, source).unwrap();

        type_statistics(db, system_path_to_file(db, filename).unwrap())
    }

    #[test]
    fn all_static() {
        let mut db = setup_db();

        let stats = get_stats(&mut db, "src/foo.py", "1");

        assert_eq!(stats.0, FxHashMap::from_iter([(Type::IntLiteral(1), 1)]));
    }

    #[test]
    fn todo_and_expression_count() {
        let mut db = setup_db();

        let stats = get_stats(
            &mut db,
            "src/foo.py",
            r#"
                x = [x for x in [1]]
            "#,
        );

        assert_eq!(stats.todo_count(), 4);
        assert_eq!(stats.expression_count(), 6);
    }
}
