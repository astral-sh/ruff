use salsa::DbWithJar;

use red_knot_module_resolver::Db as ResolverDb;
use ruff_db::{Db as SourceDb, Upcast};

use crate::builtins::builtins_scope;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::ScopeId;
use crate::semantic_index::{global_scope, semantic_index, symbol_table, use_def_map};
use crate::types::{
    infer_definition_types, infer_expression_types, infer_scope_types, ClassType, FunctionType,
    IntersectionType, UnionType,
};

#[salsa::jar(db=Db)]
pub struct Jar(
    ScopeId<'_>,
    Definition<'_>,
    Expression<'_>,
    FunctionType<'_>,
    ClassType<'_>,
    UnionType<'_>,
    IntersectionType<'_>,
    symbol_table,
    use_def_map,
    global_scope,
    semantic_index,
    infer_definition_types,
    infer_expression_types,
    infer_scope_types,
    builtins_scope,
);

/// Database giving access to semantic information about a Python program.
pub trait Db: SourceDb + ResolverDb + DbWithJar<Jar> + Upcast<dyn ResolverDb> {}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use salsa::DebugWithDb;

    use red_knot_module_resolver::{vendored_typeshed_stubs, Db as ResolverDb, Jar as ResolverJar};
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};

    use super::{Db, Jar};

    #[salsa::db(Jar, ResolverJar, SourceJar)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: std::sync::Arc<std::sync::Mutex<Vec<salsa::Event>>>,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: vendored_typeshed_stubs().snapshot(),
                events: std::sync::Arc::default(),
                files: Files::default(),
            }
        }

        /// Takes the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = Arc::get_mut(&mut self.events).expect("no pending salsa snapshots");

            let events = inner.get_mut().unwrap();
            std::mem::take(&mut *events)
        }

        /// Clears the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn clear_salsa_events(&mut self) {
            self.take_salsa_events();
        }
    }

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
        }
    }

    impl SourceDb for TestDb {
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }
    }

    impl Upcast<dyn SourceDb> for TestDb {
        fn upcast(&self) -> &(dyn SourceDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
            self
        }
    }

    impl Upcast<dyn ResolverDb> for TestDb {
        fn upcast(&self) -> &(dyn ResolverDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn ResolverDb + 'static) {
            self
        }
    }

    impl red_knot_module_resolver::Db for TestDb {}
    impl Db for TestDb {}

    impl salsa::Database for TestDb {
        fn salsa_event(&self, event: salsa::Event) {
            tracing::trace!("event: {:?}", event.debug(self));
            let mut events = self.events.lock().unwrap();
            events.push(event);
        }
    }

    impl salsa::ParallelDatabase for TestDb {
        fn snapshot(&self) -> salsa::Snapshot<Self> {
            salsa::Snapshot::new(Self {
                storage: self.storage.snapshot(),
                files: self.files.snapshot(),
                system: self.system.snapshot(),
                vendored: self.vendored.snapshot(),
                events: self.events.clone(),
            })
        }
    }
}
