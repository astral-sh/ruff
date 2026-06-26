use crate::environment::InferenceSettings;
use crate::{Db, platform::PythonPlatform};

use ruff_db::{files::File, system::SystemPath};
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{ProgramFile, ResolverProgram, SearchPaths};
use ty_site_packages::{PythonVersionSource, PythonVersionWithSource};

// Re-export the misconfiguration strategy types from ty_module_resolver.
pub use ty_module_resolver::{FallibleStrategy, MisconfigurationStrategy, UseDefaultStrategy};

/// The semantic Python environment used to analyze source files.
///
/// Programs are immutable and canonicalized. Changing any semantic setting creates a different
/// program instead of invalidating queries for the existing program.
#[salsa::interned(
    debug,
    heap_size = ruff_memory_usage::heap_size
)]
pub struct Program<'db> {
    resolver_program: ResolverProgram<'db>,

    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(ref)]
    pub settings: InferenceSettings,
}

// The Salsa allocation is tracked separately.
impl get_size2::GetSize for Program<'_> {}

impl<'db> Program<'db> {
    pub fn create(db: &'db dyn Db, settings: &ProgramSettings) -> Self {
        Self::from_settings(db, settings, &InferenceSettings::default())
    }

    pub fn from_settings(
        db: &'db dyn Db,
        settings: &ProgramSettings,
        inference_settings: &InferenceSettings,
    ) -> Self {
        let resolver_program =
            ResolverProgram::create(db, settings.python_version.version, &settings.search_paths);
        Self::new(
            db,
            resolver_program,
            &settings.python_platform,
            inference_settings,
        )
    }

    #[must_use]
    pub fn with_inference_settings(self, db: &'db dyn Db, settings: InferenceSettings) -> Self {
        Self::new(
            db,
            self.resolver(db),
            self.python_platform(db).clone(),
            settings,
        )
    }

    pub fn custom_stdlib_search_path(self, db: &'db dyn Db) -> Option<&'db SystemPath> {
        self.search_paths(db).custom_stdlib()
    }

    pub fn python_version(self, db: &'db dyn Db) -> PythonVersion {
        self.resolver(db).python_version(db)
    }

    pub fn search_paths(self, db: &'db dyn Db) -> &'db SearchPaths {
        self.resolver(db).search_paths(db)
    }

    /// Returns the module-resolution projection of this program.
    pub fn resolver(self, db: &'db dyn Db) -> ResolverProgram<'db> {
        self.resolver_program(db)
    }

    pub fn file(self, db: &'db dyn Db, file: File) -> ProgramFile<'db> {
        ProgramFile::new(db, self.resolver(db), file)
    }

    /// Captures the settings needed to recreate this program against another database view.
    ///
    /// This is intended for orchestration that mutates the database. Holding a Salsa database
    /// snapshot merely to extend a [`Program`] handle's lifetime prevents those writes.
    pub fn snapshot(self, db: &'db dyn Db) -> ProgramSnapshot {
        ProgramSnapshot {
            program_settings: ProgramSettings {
                python_version: PythonVersionWithSource {
                    version: self.python_version(db),
                    source: PythonVersionSource::default(),
                },
                python_platform: self.python_platform(db).clone(),
                search_paths: self.search_paths(db).clone(),
            },
            inference_settings: self.settings(db).clone(),
        }
    }
}

/// An owned description of a [`Program`] for database-mutating orchestration.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct ProgramSnapshot {
    program_settings: ProgramSettings,
    inference_settings: InferenceSettings,
}

impl ProgramSnapshot {
    pub fn program<'db>(&self, db: &'db dyn Db) -> Program<'db> {
        Program::from_settings(db, &self.program_settings, &self.inference_settings)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct ProgramSettings {
    pub python_version: PythonVersionWithSource,
    pub python_platform: PythonPlatform,
    pub search_paths: SearchPaths,
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::PythonVersion;
    use ty_module_resolver::SearchPaths;
    use ty_site_packages::{PythonVersionSource, PythonVersionWithSource};

    use super::{Program, ProgramSettings};
    use crate::db::tests::TestDbBuilder;

    fn settings(db: &dyn crate::Db, version: PythonVersion) -> ProgramSettings {
        ProgramSettings {
            python_version: PythonVersionWithSource {
                version,
                source: PythonVersionSource::default(),
            },
            python_platform: crate::platform::PythonPlatform::default(),
            search_paths: SearchPaths::empty(db.vendored()),
        }
    }

    #[test]
    fn programs_are_canonicalized_by_semantic_settings() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let first_settings = settings(&db, PythonVersion::default());
        let first = Program::create(&db, &first_settings);

        let mut different_provenance = first_settings;
        different_provenance.python_version.source = PythonVersionSource::Cli;
        let second = Program::create(&db, &different_provenance);

        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn changing_settings_creates_an_independent_program() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let original_settings = settings(&db, PythonVersion::default());
        let original = Program::create(&db, &original_settings);

        let mut changed_settings = original_settings;
        changed_settings.python_version = PythonVersionWithSource {
            version: PythonVersion::PY311,
            source: PythonVersionSource::Cli,
        };
        let changed = Program::create(&db, &changed_settings);

        assert_ne!(changed, original);
        assert_eq!(original.python_version(&db), PythonVersion::default());
        assert_eq!(changed.python_version(&db), PythonVersion::PY311);
        Ok(())
    }
}
