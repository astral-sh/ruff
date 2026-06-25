use std::sync::{Arc, RwLock};

use ruff_db::files::File;
use ruff_db::parsed::VersionedFile;
use salsa::{Durability, Setter};
use ty_module_resolver::{ModuleGlobSet, ProgramFile};

use crate::Db;
use crate::program::Program;

/// Settings that can change inferred types and must therefore participate in inference keys.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct InferenceSettings {
    pub replace_imports_with_any: ModuleGlobSet,
}

impl Default for InferenceSettings {
    fn default() -> Self {
        Self {
            replace_imports_with_any: ModuleGlobSet::empty(),
        }
    }
}

/// A Python program plus the settings that can change its inferred type graph.
#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub struct InferenceEnvironment {
    pub program: Program,

    #[returns(ref)]
    pub settings: InferenceSettings,
}

// The Salsa allocation is tracked separately.
impl get_size2::GetSize for InferenceEnvironment {}

#[salsa::input(singleton)]
struct DefaultInferenceEnvironment {
    environment: InferenceEnvironment,
}

impl InferenceEnvironment {
    pub fn create(db: &dyn Db, program: Program, settings: InferenceSettings) -> Self {
        Self::builder(program, settings)
            .durability(Durability::HIGH)
            .new(db)
    }

    pub fn get(db: &dyn Db) -> Self {
        DefaultInferenceEnvironment::get(db).environment(db)
    }

    pub fn ensure_default(db: &dyn Db, program: Program) -> Self {
        if let Some(default) = DefaultInferenceEnvironment::try_get(db) {
            default.environment(db)
        } else {
            let environment = Self::create(db, program, InferenceSettings::default());
            let _ = DefaultInferenceEnvironment::builder(environment)
                .durability(Durability::HIGH)
                .new(db);
            environment
        }
    }

    pub fn set_default(db: &mut dyn Db, environment: Self) {
        match DefaultInferenceEnvironment::try_get(db) {
            Some(default) if default.environment(db) != environment => {
                default.set_environment(db).to(environment);
            }
            Some(_) => {}
            None => {
                let _ = DefaultInferenceEnvironment::builder(environment)
                    .durability(Durability::HIGH)
                    .new(db);
            }
        }
    }
}

#[derive(Clone)]
struct InferenceEnvironmentEntry {
    program: Program,
    settings: InferenceSettings,
    environment: InferenceEnvironment,
    users: usize,
}

/// Registry for canonical, actively used inference environments.
#[derive(Clone, Default)]
pub struct InferenceEnvironments {
    entries: Arc<RwLock<Vec<InferenceEnvironmentEntry>>>,
}

impl InferenceEnvironments {
    pub fn acquire(
        &self,
        db: &dyn Db,
        program: Program,
        settings: InferenceSettings,
    ) -> InferenceEnvironment {
        let mut entries = self.entries.write().unwrap();
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.program == program && entry.settings == settings)
        {
            entry.users += 1;
            return entry.environment;
        }

        let environment = InferenceEnvironment::create(db, program, settings.clone());
        entries.push(InferenceEnvironmentEntry {
            program,
            settings,
            environment,
            users: 1,
        });
        environment
    }

    pub fn reconfigure(
        &self,
        db: &mut dyn Db,
        environment: InferenceEnvironment,
        program: Program,
        settings: InferenceSettings,
    ) -> InferenceEnvironment {
        let mut entries = self.entries.write().unwrap();
        let Some(current_index) = entries
            .iter()
            .position(|entry| entry.environment == environment)
        else {
            drop(entries);
            return self.acquire(db, program, settings);
        };

        if entries[current_index].program == program && entries[current_index].settings == settings
        {
            return environment;
        }

        if let Some(target_index) = entries
            .iter()
            .position(|entry| entry.program == program && entry.settings == settings)
        {
            entries[current_index].users -= 1;
            entries[target_index].users += 1;
            return entries[target_index].environment;
        }

        if entries[current_index].users == 1 {
            entries[current_index].program = program;
            entries[current_index].settings = settings.clone();
            drop(entries);
            if environment.program(db) != program {
                environment.set_program(db).to(program);
            }
            if environment.settings(db) != &settings {
                environment.set_settings(db).to(settings);
            }
            return environment;
        }

        entries[current_index].users -= 1;
        drop(entries);
        self.acquire(db, program, settings)
    }

    pub fn release(&self, environment: InferenceEnvironment) {
        let mut entries = self.entries.write().unwrap();
        if let Some(index) = entries
            .iter()
            .position(|entry| entry.environment == environment)
        {
            if entries[index].users == 1 {
                entries.swap_remove(index);
            } else {
                entries[index].users -= 1;
            }
        }
    }
}

/// A physical file interpreted in one inference environment.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct AnalysisFile<'db> {
    pub environment: InferenceEnvironment,
    pub file: File,
}

// The Salsa allocation is tracked separately.
impl get_size2::GetSize for AnalysisFile<'_> {}

impl<'db> AnalysisFile<'db> {
    pub fn from_default(db: &'db dyn Db, file: File) -> Self {
        Self::new(db, InferenceEnvironment::get(db), file)
    }

    pub fn program(self, db: &'db dyn Db) -> Program {
        self.environment(db).program(db)
    }

    pub fn program_file(self, db: &'db dyn Db) -> ProgramFile<'db> {
        self.program(db).file(db, self.file(db))
    }

    pub fn versioned_file(self, db: &'db dyn Db) -> VersionedFile<'db> {
        self.program(db).versioned_file(db, self.file(db))
    }
}

#[cfg(test)]
mod tests {
    use super::{InferenceEnvironments, InferenceSettings};
    use crate::{db::tests::TestDbBuilder, program::Program};

    #[test]
    fn registry_reuses_equal_environments_and_detaches_shared_updates() -> anyhow::Result<()> {
        let mut db = TestDbBuilder::new().build()?;
        let environments = InferenceEnvironments::default();
        let program = Program::get(&db);
        let shared = environments.acquire(&db, program, InferenceSettings::default());
        assert_eq!(
            shared,
            environments.acquire(&db, program, InferenceSettings::default())
        );

        let changed = InferenceSettings {
            replace_imports_with_any: ty_module_resolver::ModuleGlobSet::from_patterns([
                "dependency",
            ])?,
        };
        let detached = environments.reconfigure(&mut db, shared, program, changed.clone());

        assert_ne!(detached, shared);
        assert_eq!(shared.settings(&db), &InferenceSettings::default());
        assert_eq!(detached.settings(&db), &changed);
        Ok(())
    }
}
