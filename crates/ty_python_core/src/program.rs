use std::sync::{Arc, RwLock};

use crate::environment::InferenceEnvironment;
use crate::{Db, platform::PythonPlatform};

use ruff_db::{files::File, parsed::VersionedFile, system::SystemPath};
use ruff_python_ast::PythonVersion;
use salsa::Durability;
use salsa::Setter;
use ty_module_resolver::{ProgramFile, ResolverProgram, SearchPaths};
use ty_site_packages::PythonVersionWithSource;

// Re-export the misconfiguration strategy types from ty_module_resolver.
pub use ty_module_resolver::{FallibleStrategy, MisconfigurationStrategy, UseDefaultStrategy};

#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub struct Program {
    #[returns(ref)]
    pub python_version_with_source: PythonVersionWithSource,

    #[returns(ref)]
    pub python_platform: PythonPlatform,

    pub resolver: ResolverProgram,
}

// The Salsa allocation is tracked separately.
impl get_size2::GetSize for Program {}

/// Compatibility adapter for entry points that operate on a single default environment.
///
/// Multi-environment queries must carry a [`Program`] explicitly instead of reading this input.
#[salsa::input(singleton, heap_size=ruff_memory_usage::heap_size)]
struct DefaultProgram {
    program: Program,
}

impl Program {
    pub fn get(db: &dyn Db) -> Self {
        DefaultProgram::get(db).program(db)
    }

    pub fn try_get(db: &dyn Db) -> Option<Self> {
        DefaultProgram::try_get(db).map(|default| default.program(db))
    }

    pub fn init_or_update(db: &mut dyn Db, settings: ProgramSettings) -> Self {
        match Self::try_get(db) {
            Some(program) => {
                program.update_from_settings(db, settings);
                program
            }
            None => Self::from_settings(db, settings),
        }
    }

    pub fn from_settings(db: &dyn Db, settings: ProgramSettings) -> Self {
        let program = Self::create(db, settings);
        if DefaultProgram::try_get(db).is_none() {
            let _ = DefaultProgram::builder(program)
                .durability(Durability::HIGH)
                .new(db);
            ResolverProgram::ensure_default(db, program.resolver(db));
            InferenceEnvironment::ensure_default(db, program);
        }
        program
    }

    /// Creates a non-default program for use by an explicitly contextualized analysis.
    pub fn create(db: &dyn Db, settings: ProgramSettings) -> Self {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        let resolver = ResolverProgram::create(db, python_version.version, search_paths);

        Program::builder(python_version, python_platform, resolver)
            .durability(Durability::HIGH)
            .new(db)
    }

    pub fn set_default(db: &mut dyn Db, program: Self) {
        match DefaultProgram::try_get(db) {
            Some(default) if default.program(db) != program => {
                default.set_program(db).to(program);
            }
            Some(_) => {}
            None => {
                let _ = DefaultProgram::builder(program)
                    .durability(Durability::HIGH)
                    .new(db);
            }
        }

        let resolver = program.resolver(db);
        ResolverProgram::set_default(db, resolver);
    }

    pub fn python_version(self, db: &dyn Db) -> PythonVersion {
        self.resolver(db).python_version(db)
    }

    pub fn update_from_settings(self, db: &mut dyn Db, settings: ProgramSettings) {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        self.resolver(db)
            .update(db, python_version.version, search_paths);

        if &python_platform != self.python_platform(db) {
            tracing::debug!("Updating python platform: `{python_platform:?}`");
            self.set_python_platform(db).to(python_platform);
        }

        if &python_version != self.python_version_with_source(db) {
            tracing::debug!(
                "Updating python version: Python {version}",
                version = python_version.version
            );
            self.set_python_version_with_source(db).to(python_version);
        }
    }

    /// Permanently freezes all program inputs.
    pub fn freeze(self, db: &mut dyn Db) {
        let durability = Durability::NEVER_CHANGE;
        let python_version = self.python_version_with_source(db).clone();
        let python_platform = self.python_platform(db).clone();
        let search_paths = self.search_paths(db).clone();

        self.set_python_version_with_source(db)
            .with_durability(durability)
            .to(python_version);
        self.set_python_platform(db)
            .with_durability(durability)
            .to(python_platform);
        self.set_search_paths(db)
            .with_durability(durability)
            .to(search_paths);
    }

    pub fn custom_stdlib_search_path(self, db: &dyn Db) -> Option<&SystemPath> {
        self.search_paths(db).custom_stdlib()
    }

    pub fn search_paths(self, db: &dyn Db) -> &SearchPaths {
        self.resolver(db).search_paths(db)
    }

    pub fn file(self, db: &dyn Db, file: File) -> ProgramFile<'_> {
        ProgramFile::new(db, self.resolver(db), file)
    }

    pub fn versioned_file(self, db: &dyn Db, file: File) -> VersionedFile<'_> {
        VersionedFile::new(db, file, self.python_version(db))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramSettings {
    pub python_version: PythonVersionWithSource,
    pub python_platform: PythonPlatform,
    pub search_paths: SearchPaths,
}

#[derive(Clone)]
struct ProgramEntry {
    settings: ProgramSettings,
    program: Program,
    users: usize,
}

/// Registry of the active Python environments in a Salsa database.
///
/// The registry deliberately uses a small linear collection: projects are expected to have few
/// simultaneous environments, and [`ProgramSettings`] contains normalized values that are
/// comparable but not naturally hashable. Clones share the same registry.
#[derive(Clone, Default)]
pub struct Programs {
    entries: Arc<RwLock<Vec<ProgramEntry>>>,
}

impl Programs {
    /// Acquires a program for `settings`, reusing an equal active environment when possible.
    pub fn acquire(&self, db: &dyn Db, settings: ProgramSettings) -> Program {
        let mut entries = self.entries.write().unwrap();
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.settings.semantically_eq(&settings))
        {
            entry.users += 1;
            return entry.program;
        }

        let program = Program::create(db, settings.clone());
        entries.push(ProgramEntry {
            settings,
            program,
            users: 1,
        });
        program
    }

    /// Moves one user of `program` to `settings`.
    ///
    /// A uniquely owned program is updated in place to preserve its Salsa identity. A shared
    /// program is detached first so that changing one environment cannot invalidate its peers.
    pub fn reconfigure(
        &self,
        db: &mut dyn Db,
        program: Program,
        settings: ProgramSettings,
    ) -> Program {
        let mut entries = self.entries.write().unwrap();
        let Some(current_index) = entries.iter().position(|entry| entry.program == program) else {
            drop(entries);
            return self.acquire(db, settings);
        };

        if entries[current_index].settings.semantically_eq(&settings) {
            // Provenance is diagnostic-only, so updating it must not fork the semantic program.
            // Keep it current for the legacy single-environment diagnostic entry points.
            if entries[current_index].settings.python_version.source
                != settings.python_version.source
            {
                program
                    .set_python_version_with_source(db)
                    .to(settings.python_version.clone());
                entries[current_index].settings.python_version = settings.python_version;
            }
            return program;
        }

        if let Some(target_index) = entries
            .iter()
            .position(|entry| entry.settings.semantically_eq(&settings))
        {
            entries[current_index].users -= 1;
            entries[target_index].users += 1;
            return entries[target_index].program;
        }

        if entries[current_index].users == 1 {
            entries[current_index].settings = settings.clone();
            drop(entries);
            program.update_from_settings(db, settings);
            return program;
        }

        entries[current_index].users -= 1;
        drop(entries);
        self.acquire(db, settings)
    }

    /// Releases one registry user of `program`.
    pub fn release(&self, program: Program) {
        let mut entries = self.entries.write().unwrap();
        if let Some(index) = entries.iter().position(|entry| entry.program == program) {
            if entries[index].users == 1 {
                entries.swap_remove(index);
            } else {
                entries[index].users -= 1;
            }
        }
    }
}

impl ProgramSettings {
    /// Compares only values that can affect Python semantics.
    ///
    /// Configuration provenance must not fork the type-inference graph.
    fn semantically_eq(&self, other: &Self) -> bool {
        self.python_version.version == other.python_version.version
            && self.python_platform == other.python_platform
            && self.search_paths == other.search_paths
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::PythonVersion;
    use ty_site_packages::{PythonVersionSource, PythonVersionWithSource};

    use super::{Program, ProgramSettings, Programs};
    use crate::db::tests::TestDbBuilder;

    fn settings(db: &dyn crate::Db, program: Program) -> ProgramSettings {
        ProgramSettings {
            python_version: program.python_version_with_source(db).clone(),
            python_platform: program.python_platform(db).clone(),
            search_paths: program.search_paths(db).clone(),
        }
    }

    #[test]
    fn registry_reuses_semantically_equal_programs() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let programs = Programs::default();
        let default = Program::get(&db);
        let first_settings = settings(&db, default);
        let first = programs.acquire(&db, first_settings.clone());

        let mut different_provenance = first_settings;
        different_provenance.python_version.source = PythonVersionSource::Cli;
        let second = programs.acquire(&db, different_provenance);

        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn reconfiguring_shared_program_detaches() -> anyhow::Result<()> {
        let mut db = TestDbBuilder::new().build()?;
        let programs = Programs::default();
        let default = Program::get(&db);
        let original_settings = settings(&db, default);
        let shared = programs.acquire(&db, original_settings.clone());
        assert_eq!(shared, programs.acquire(&db, original_settings.clone()));

        let mut changed_settings = original_settings;
        changed_settings.python_version = PythonVersionWithSource {
            version: PythonVersion::PY311,
            source: PythonVersionSource::Cli,
        };
        let detached = programs.reconfigure(&mut db, shared, changed_settings);

        assert_ne!(detached, shared);
        assert_eq!(shared.python_version(&db), PythonVersion::default());
        assert_eq!(detached.python_version(&db), PythonVersion::PY311);
        Ok(())
    }
}
