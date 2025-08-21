use anyhow::{Context, Result};
use std::sync::Arc;
use zip::CompressionMethod;

use ruff_db::Db as SourceDb;
use ruff_db::files::{File, Files};
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use ruff_db::vendored::{VendoredFileSystem, VendoredFileSystemBuilder};
use ruff_python_ast::PythonVersion;
use ty_python_semantic::lint::{LintRegistry, RuleSelection};
use ty_python_semantic::{
    Db, Program, ProgramSettings, PythonEnvironment, PythonPlatform, PythonVersionSource,
    PythonVersionWithSource, SearchPathSettings, SysPrefixPathOrigin, default_lint_registry,
};

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
        src_roots: Vec<SystemPathBuf>,
        python_version: PythonVersion,
        venv_path: Option<SystemPathBuf>,
    ) -> Result<Self> {
        let db = Self::default();
        let mut search_paths = SearchPathSettings::new(src_roots);
        // TODO: Consider calling `PythonEnvironment::discover` if the `venv_path` is not provided.
        if let Some(venv_path) = venv_path {
            let environment =
                PythonEnvironment::new(venv_path, SysPrefixPathOrigin::PythonCliFlag, db.system())?;
            search_paths.site_packages_paths = environment
                .site_packages_paths(db.system())
                .context("Failed to discover the site-packages directory")?
                .into_vec();
        }
        let search_paths = search_paths
            .to_search_paths(db.system(), db.vendored())
            .context("Invalid search path settings")?;

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersionWithSource {
                    version: python_version,
                    source: PythonVersionSource::default(),
                },
                python_platform: PythonPlatform::default(),
                search_paths,
            },
        );

        Ok(db)
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

    fn python_version(&self) -> PythonVersion {
        Program::get(self).python_version(self)
    }
}

#[salsa::db]
impl Db for ModuleDb {
    fn should_check_file(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }

    fn rule_selection(&self, _file: File) -> &RuleSelection {
        &self.rule_selection
    }

    fn lint_registry(&self) -> &LintRegistry {
        default_lint_registry()
    }
}

#[salsa::db]
impl salsa::Database for ModuleDb {}
