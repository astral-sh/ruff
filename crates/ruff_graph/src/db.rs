use anyhow::Result;
use std::sync::Arc;
use zip::CompressionMethod;

use red_knot_python_semantic::lint::{LintRegistry, RuleSelection};
use red_knot_python_semantic::{
    default_lint_registry, Db, Program, ProgramSettings, PythonPlatform, PythonVersion,
    SearchPathSettings,
};
use ruff_db::files::{File, Files};
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use ruff_db::vendored::{VendoredFileSystem, VendoredFileSystemBuilder};
use ruff_db::{Db as SourceDb, Upcast};

static EMPTY_VENDORED: std::sync::LazyLock<VendoredFileSystem> = std::sync::LazyLock::new(|| {
    let mut builder = VendoredFileSystemBuilder::new(CompressionMethod::Stored);
    builder.add_file("stdlib/VERSIONS", "\n").unwrap();
    builder.finish().unwrap()
});

#[salsa::db]
#[derive(Default, Clone)]
pub struct ModuleDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: OsSystem,
    rule_selection: Arc<RuleSelection>,
}

impl ModuleDb {
    /// Initialize a [`ModuleDb`] from the given source root.
    pub fn from_src_roots(
        mut src_roots: impl Iterator<Item = SystemPathBuf>,
        python_version: PythonVersion,
    ) -> Result<Self> {
        let search_paths = {
            // Use the first source root.
            let src_root = src_roots
                .next()
                .ok_or_else(|| anyhow::anyhow!("No source roots provided"))?;

            let mut search_paths = SearchPathSettings::new(src_root);

            // Add the remaining source roots as extra paths.
            search_paths.extra_paths.extend(src_roots);

            search_paths
        };

        let db = Self::default();
        Program::from_settings(
            &db,
            &ProgramSettings {
                python_version,
                python_platform: PythonPlatform::default(),
                search_paths,
            },
        )?;

        Ok(db)
    }
}

impl Upcast<dyn SourceDb> for ModuleDb {
    fn upcast(&self) -> &(dyn SourceDb + 'static) {
        self
    }
    fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
        self
    }
}

#[salsa::db]
impl SourceDb for ModuleDb {
    fn vendored(&self) -> &VendoredFileSystem {
        &EMPTY_VENDORED
    }

    fn system(&self) -> &dyn System {
        &self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }
}

#[salsa::db]
impl Db for ModuleDb {
    fn is_file_open(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }

    fn rule_selection(&self) -> &RuleSelection {
        &self.rule_selection
    }

    fn lint_registry(&self) -> &LintRegistry {
        default_lint_registry()
    }
}

#[salsa::db]
impl salsa::Database for ModuleDb {
    fn salsa_event(&self, _event: &dyn Fn() -> salsa::Event) {}
}
