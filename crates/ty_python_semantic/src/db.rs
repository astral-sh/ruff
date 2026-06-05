use std::cell::{Cell, RefCell};
use std::panic::{RefUnwindSafe, UnwindSafe};

use crate::AnalysisSettings;
use crate::lint::{LintRegistry, RuleSelection};
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::File;
use ruff_index::IndexSlice;
use rustc_hash::FxHashMap;
use salsa::plumbing::AsId;
use ty_python_core::Db as PythonCoreDb;
use ty_python_core::predicate::{Predicate, ScopedPredicateId};
use ty_python_core::{LoopToken, Truthiness};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LoopHeaderPredicateCacheKey {
    predicates: usize,
    predicate: ScopedPredicateId,
}

#[derive(Debug, Clone, Copy)]
struct LoopHeaderPredicateCacheEntry {
    truthiness: Truthiness,
    generation: u64,
    persistence: LoopHeaderPredicateCachePersistence,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum LoopHeaderPredicateCachePersistence {
    #[default]
    None,
    Global,
    Loop(salsa::Id),
}

impl LoopHeaderPredicateCachePersistence {
    fn for_context(loop_token: Option<LoopToken<'_>>, persistent: bool) -> Self {
        if !persistent {
            Self::None
        } else if let Some(loop_token) = loop_token {
            Self::Loop(loop_token.as_id())
        } else {
            Self::Global
        }
    }

    fn matches(self, loop_token: Option<LoopToken<'_>>) -> bool {
        match self {
            Self::None => false,
            Self::Global => true,
            Self::Loop(id) => loop_token.is_some_and(|loop_token| loop_token.as_id() == id),
        }
    }
}

/// Predicate results cached for one dynamic loop-header analysis.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct LoopHeaderPredicateCache {
    entries: RefCell<Option<FxHashMap<LoopHeaderPredicateCacheKey, LoopHeaderPredicateCacheEntry>>>,
    generation: Cell<u64>,
    // Nested semantic queries can evaluate unrelated predicate tables while loop analysis is
    // active. Only tables explicitly entered through `with_scope` may observe this cache.
    active_predicate_tables: RefCell<Vec<usize>>,
}

impl Clone for LoopHeaderPredicateCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

// The cache is cleared by `with_scope`'s guard if predicate analysis unwinds.
impl RefUnwindSafe for LoopHeaderPredicateCache {}
impl UnwindSafe for LoopHeaderPredicateCache {}

impl LoopHeaderPredicateCache {
    pub(crate) fn with_scope<T>(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'_>>,
        f: impl FnOnce() -> T,
    ) -> T {
        let predicate_table = predicate_table_key(predicates);
        let owner = {
            let mut active_predicate_tables = self.active_predicate_tables.borrow_mut();
            let owner = active_predicate_tables.is_empty();
            active_predicate_tables.push(predicate_table);
            owner
        };
        if owner {
            let mut entries = self.entries.borrow_mut();
            debug_assert!(entries.is_none());
            *entries = Some(FxHashMap::default());
        }
        let _guard = LoopHeaderPredicateCacheGuard {
            cache: self,
            predicate_table,
            owner,
        };
        f()
    }

    pub(crate) fn is_active_for(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'_>>,
    ) -> bool {
        let predicate_table = predicate_table_key(predicates);
        self.active_predicate_tables
            .borrow()
            .contains(&predicate_table)
    }

    pub(crate) fn get_globally_persistent(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'_>>,
        predicate: ScopedPredicateId,
    ) -> Option<Truthiness> {
        let entries = self.entries.borrow();
        let entry = entries.as_ref()?.get(&LoopHeaderPredicateCacheKey {
            predicates: predicate_table_key(predicates),
            predicate,
        })?;
        (entry.persistence == LoopHeaderPredicateCachePersistence::Global)
            .then_some(entry.truthiness)
    }

    pub(crate) fn get_or_prepare(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'_>>,
        loop_token: Option<LoopToken<'_>>,
        predicate: ScopedPredicateId,
        cycle_initial: Truthiness,
        persistent: bool,
    ) -> Option<Truthiness> {
        let generation = self.generation.get();
        let predicate_key = LoopHeaderPredicateCacheKey {
            predicates: predicate_table_key(predicates),
            predicate,
        };
        let mut entries = self.entries.borrow_mut();
        let entries = entries.as_mut()?;
        get_or_prepare_cache_entry(
            entries,
            predicate_key,
            loop_token,
            generation,
            cycle_initial,
            persistent,
        )
    }

    pub(crate) fn insert(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'_>>,
        loop_token: Option<LoopToken<'_>>,
        predicate: ScopedPredicateId,
        truthiness: Truthiness,
        persistent: bool,
    ) {
        if let Some(entries) = self.entries.borrow_mut().as_mut() {
            let generation = self.generation.get();
            let predicate_key = LoopHeaderPredicateCacheKey {
                predicates: predicate_table_key(predicates),
                predicate,
            };
            insert_cache_entry(
                entries,
                predicate_key,
                loop_token,
                generation,
                truthiness,
                persistent,
            );
        }
    }

    pub(crate) fn mark_persistent(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'_>>,
        loop_token: Option<LoopToken<'_>>,
        predicate: ScopedPredicateId,
    ) {
        let mut entries = self.entries.borrow_mut();
        let Some(entries) = entries.as_mut() else {
            return;
        };
        let predicate_key = LoopHeaderPredicateCacheKey {
            predicates: predicate_table_key(predicates),
            predicate,
        };
        if let Some(entry) = entries.get_mut(&predicate_key) {
            let persistence = LoopHeaderPredicateCachePersistence::for_context(loop_token, true);
            if entry.persistence != LoopHeaderPredicateCachePersistence::Global {
                entry.persistence = persistence;
            }
        }
    }

    pub(crate) fn next_cycle_iteration(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}

fn get_or_prepare_cache_entry(
    entries: &mut FxHashMap<LoopHeaderPredicateCacheKey, LoopHeaderPredicateCacheEntry>,
    key: LoopHeaderPredicateCacheKey,
    loop_token: Option<LoopToken<'_>>,
    generation: u64,
    cycle_initial: Truthiness,
    persistent: bool,
) -> Option<Truthiness> {
    match entries.entry(key) {
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            let entry = entry.get_mut();
            if entry.persistence.matches(loop_token) || entry.generation == generation {
                Some(entry.truthiness)
            } else {
                entry.generation = generation;
                entry.persistence = LoopHeaderPredicateCachePersistence::None;
                None
            }
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(LoopHeaderPredicateCacheEntry {
                truthiness: cycle_initial,
                generation,
                persistence: LoopHeaderPredicateCachePersistence::for_context(
                    loop_token, persistent,
                ),
            });
            None
        }
    }
}

fn insert_cache_entry(
    entries: &mut FxHashMap<LoopHeaderPredicateCacheKey, LoopHeaderPredicateCacheEntry>,
    key: LoopHeaderPredicateCacheKey,
    loop_token: Option<LoopToken<'_>>,
    generation: u64,
    truthiness: Truthiness,
    persistent: bool,
) {
    let persistence = LoopHeaderPredicateCachePersistence::for_context(loop_token, persistent);
    entries
        .entry(key)
        .and_modify(|entry| {
            entry.truthiness = truthiness;
            entry.generation = generation;
            if entry.persistence != LoopHeaderPredicateCachePersistence::Global {
                entry.persistence = persistence;
            }
        })
        .or_insert(LoopHeaderPredicateCacheEntry {
            truthiness,
            generation,
            persistence,
        });
}

struct LoopHeaderPredicateCacheGuard<'db> {
    cache: &'db LoopHeaderPredicateCache,
    predicate_table: usize,
    owner: bool,
}

impl Drop for LoopHeaderPredicateCacheGuard<'_> {
    fn drop(&mut self) {
        let predicate_table = self.cache.active_predicate_tables.borrow_mut().pop();
        debug_assert_eq!(predicate_table, Some(self.predicate_table));
        if self.owner {
            debug_assert!(self.cache.active_predicate_tables.borrow().is_empty());
            self.cache.entries.borrow_mut().take();
        }
    }
}

fn predicate_table_key(predicates: &IndexSlice<ScopedPredicateId, Predicate<'_>>) -> usize {
    predicates.raw.as_ptr() as usize
}

/// Database giving access to semantic information about a Python program.
#[salsa::db]
pub trait Db: PythonCoreDb {
    fn check_file(&self, file: File) -> Vec<Diagnostic>;

    /// Resolves the rule selection for a given file.
    fn rule_selection(&self, file: File) -> &RuleSelection;

    fn lint_registry(&self) -> &LintRegistry;

    fn analysis_settings(&self, file: File) -> &AnalysisSettings;

    /// Whether ty is running with logging verbosity INFO or higher (`-v` or more).
    fn verbose(&self) -> bool;

    fn dyn_clone(&self) -> Box<dyn Db>;

    #[doc(hidden)]
    fn loop_header_predicate_cache(&self) -> &LoopHeaderPredicateCache;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    use anyhow::Context;
    use ty_python_core::platform::PythonPlatform;

    use crate::{check_file_unwrap, default_lint_registry};
    use ruff_db::Db as SourceDb;
    use ruff_db::files::Files;
    use ruff_db::system::{
        DbWithTestSystem, DbWithWritableSystem as _, System, SystemPath, SystemPathBuf, TestSystem,
    };
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_python_ast::PythonVersion;
    use ty_module_resolver::{Db as ModuleResolverDb, SearchPathSettings, SearchPaths};
    use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings};
    use ty_site_packages::{PythonVersionSource, PythonVersionWithSource};

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: Events,
        rule_selection: Arc<RuleSelection>,
        analysis_settings: Arc<AnalysisSettings>,
        loop_header_predicate_cache: LoopHeaderPredicateCache,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            let events = Events::default();
            Self {
                storage: salsa::Storage::new(Some(Box::new({
                    let events = events.clone();
                    move |event| {
                        tracing::trace!("event: {event:?}");
                        let mut events = events.lock().unwrap();
                        events.push(event);
                    }
                }))),
                system: TestSystem::default(),
                vendored: ty_vendored::file_system().clone(),
                events,
                files: Files::default(),
                rule_selection: Arc::new(RuleSelection::from_registry(default_lint_registry())),
                analysis_settings: AnalysisSettings::default().into(),
                loop_header_predicate_cache: LoopHeaderPredicateCache::default(),
            }
        }

        /// Takes the salsa events.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let mut events = self.events.lock().unwrap();

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

    #[salsa::db]
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

        fn python_version(&self) -> PythonVersion {
            Program::get(self).python_version(self)
        }
    }

    #[salsa::db]
    impl ty_python_core::Db for TestDb {
        fn should_check_file(&self, file: File) -> bool {
            !file.path(self).is_vendored_path()
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn check_file(&self, file: File) -> Vec<Diagnostic> {
            if !self.should_check_file(file) {
                return Vec::new();
            }

            check_file_unwrap(self, file)
        }

        fn rule_selection(&self, _file: File) -> &RuleSelection {
            &self.rule_selection
        }

        fn lint_registry(&self) -> &LintRegistry {
            default_lint_registry()
        }

        fn analysis_settings(&self, _file: File) -> &AnalysisSettings {
            &self.analysis_settings
        }

        fn verbose(&self) -> bool {
            false
        }

        fn dyn_clone(&self) -> Box<dyn crate::Db> {
            Box::new(self.clone())
        }

        fn loop_header_predicate_cache(&self) -> &LoopHeaderPredicateCache {
            &self.loop_header_predicate_cache
        }
    }

    #[salsa::db]
    impl ModuleResolverDb for TestDb {
        fn search_paths(&self) -> &SearchPaths {
            Program::get(self).search_paths(self)
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    pub(crate) struct TestDbBuilder<'a> {
        /// Target Python version
        python_version: PythonVersion,
        /// Target Python platform
        python_platform: PythonPlatform,
        /// Path and content pairs for files that should be present
        files: Vec<(&'a str, &'a str)>,
    }

    impl<'a> TestDbBuilder<'a> {
        pub(crate) fn new() -> Self {
            Self {
                python_version: PythonVersion::default(),
                python_platform: PythonPlatform::default(),
                files: vec![],
            }
        }

        pub(crate) fn with_python_version(mut self, version: PythonVersion) -> Self {
            self.python_version = version;
            self
        }

        pub(crate) fn with_python_platform(mut self, platform: PythonPlatform) -> Self {
            self.python_platform = platform;
            self
        }

        pub(crate) fn with_file(
            mut self,
            path: &'a (impl AsRef<SystemPath> + ?Sized),
            content: &'a str,
        ) -> Self {
            self.files.push((path.as_ref().as_str(), content));
            self
        }

        pub(crate) fn build(self) -> anyhow::Result<TestDb> {
            let mut db = TestDb::new();

            let src_root = SystemPathBuf::from("/src");
            db.memory_file_system().create_directory_all(&src_root)?;

            db.write_files(self.files)
                .context("Failed to write test files")?;

            Program::from_settings(
                &db,
                ProgramSettings {
                    python_version: PythonVersionWithSource {
                        version: self.python_version,
                        source: PythonVersionSource::default(),
                    },
                    python_platform: self.python_platform,
                    search_paths: SearchPathSettings::new(vec![src_root])
                        .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                        .context("Invalid search path settings")?,
                },
            );

            Ok(db)
        }
    }

    pub(crate) fn setup_db() -> TestDb {
        TestDbBuilder::new().build().expect("valid TestDb setup")
    }
}
