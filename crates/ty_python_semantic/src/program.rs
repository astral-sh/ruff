use std::sync::Arc;

use rustc_hash::{FxBuildHasher, FxHashSet};

use crate::Db;
use crate::python_platform::PythonPlatform;

use ruff_db::diagnostic::Span;
use ruff_db::files::system_path_to_file;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;
use ruff_text_size::TextRange;
use salsa::Durability;
use salsa::Setter;
pub use ty_module_resolver::{
    SearchPath, SearchPathValidationError, SearchPaths, SearchPathsBuilder, TypeshedVersions,
    vendored_typeshed_versions,
};

#[salsa::input(singleton, heap_size=ruff_memory_usage::heap_size)]
pub struct Program {
    #[returns(ref)]
    pub python_version_with_source: PythonVersionWithSource,

    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(ref)]
    pub search_paths: SearchPaths,
}

impl Program {
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
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        search_paths.try_register_static_roots(db);

        Program::builder(python_version, python_platform, search_paths)
            .durability(Durability::HIGH)
            .new(db)
    }

    pub fn python_version(self, db: &dyn Db) -> PythonVersion {
        self.python_version_with_source(db).version
    }

    pub fn update_from_settings(self, db: &mut dyn Db, settings: ProgramSettings) {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        if self.search_paths(db) != &search_paths {
            tracing::debug!("Updating search paths");
            search_paths.try_register_static_roots(db);
            self.set_search_paths(db).to(search_paths);
        }

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

    pub fn custom_stdlib_search_path(self, db: &dyn Db) -> Option<&SystemPath> {
        self.search_paths(db).custom_stdlib()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramSettings {
    pub python_version: PythonVersionWithSource,
    pub python_platform: PythonPlatform,
    pub search_paths: SearchPaths,
}

#[derive(Clone, Debug, Eq, PartialEq, Default, get_size2::GetSize)]
pub enum PythonVersionSource {
    /// Value loaded from a project's configuration file.
    ConfigFile(PythonVersionFileSource),

    /// Value loaded from the `pyvenv.cfg` file of the virtual environment.
    /// The virtual environment might have been configured, activated or inferred.
    PyvenvCfgFile(PythonVersionFileSource),

    /// Value inferred from the layout of the Python installation.
    ///
    /// This only ever applies on Unix. On Unix, the `site-packages` directory
    /// will always be at `sys.prefix/lib/pythonX.Y/site-packages`,
    /// so we can infer the Python version from the parent directory of `site-packages`.
    InstallationDirectoryLayout { site_packages_parent_dir: Box<str> },

    /// The value comes from a CLI argument, while it's left open if specified using a short argument,
    /// long argument (`--extra-paths`) or `--config key=value`.
    Cli,

    /// The value comes from the user's editor,
    /// while it's left open if specified as a setting
    /// or if the value was auto-discovered by the editor
    /// (e.g., the Python environment)
    Editor,

    /// We fell back to a default value because the value was not specified via the CLI or a config file.
    #[default]
    Default,
}

/// Information regarding the file and [`TextRange`] of the configuration
/// from which we inferred the Python version.
#[derive(Debug, PartialEq, Eq, Clone, get_size2::GetSize)]
pub struct PythonVersionFileSource {
    path: Arc<SystemPathBuf>,
    range: Option<TextRange>,
}

impl PythonVersionFileSource {
    pub fn new(path: Arc<SystemPathBuf>, range: Option<TextRange>) -> Self {
        Self { path, range }
    }

    /// Attempt to resolve a [`Span`] that corresponds to the location of
    /// the configuration setting that specified the Python version.
    ///
    /// Useful for subdiagnostics when informing the user
    /// what the inferred Python version of their project is.
    pub(crate) fn span(&self, db: &dyn Db) -> Option<Span> {
        let file = system_path_to_file(db, &*self.path).ok()?;
        Some(Span::from(file).with_optional_range(self.range))
    }
}

#[derive(Eq, PartialEq, Debug, Clone, get_size2::GetSize)]
pub struct PythonVersionWithSource {
    pub version: PythonVersion,
    pub source: PythonVersionSource,
}

impl Default for PythonVersionWithSource {
    fn default() -> Self {
        Self {
            version: PythonVersion::latest_ty(),
            source: PythonVersionSource::Default,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Copy, Clone, get_size2::GetSize)]
pub enum MisconfigurationMode {
    /// Settings Failure Is Not An Error.
    ///
    /// This is used by the default database, which we are incentivized to make infallible,
    /// while still trying to "do our best" to set things up properly where we can.
    UseDefault,
    /// Settings Failure Is An Error.
    Fail,
}

/// Configures the search paths for module resolution.
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct SearchPathSettings {
    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Vec<SystemPathBuf>,

    /// The root of the project, used for finding first-party modules.
    pub src_roots: Vec<SystemPathBuf>,

    /// Optional path to a "custom typeshed" directory on disk for us to use for standard-library types.
    /// If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
    /// bundled as a zip file in the binary
    pub custom_typeshed: Option<SystemPathBuf>,

    /// List of site packages paths to use.
    pub site_packages_paths: Vec<SystemPathBuf>,

    /// Option path to the real stdlib on the system, and not some instance of typeshed.
    ///
    /// We should ideally only ever use this for things like goto-definition,
    /// where typeshed isn't the right answer.
    pub real_stdlib_path: Option<SystemPathBuf>,

    /// How to handle apparent misconfiguration
    pub misconfiguration_mode: MisconfigurationMode,
}

impl SearchPathSettings {
    pub fn new(src_roots: Vec<SystemPathBuf>) -> Self {
        Self {
            src_roots,
            ..SearchPathSettings::empty()
        }
    }

    pub fn empty() -> Self {
        SearchPathSettings {
            src_roots: vec![],
            extra_paths: vec![],
            custom_typeshed: None,
            site_packages_paths: vec![],
            real_stdlib_path: None,
            misconfiguration_mode: MisconfigurationMode::Fail,
        }
    }

    pub fn to_search_paths(
        &self,
        system: &dyn System,
        vendored: &VendoredFileSystem,
    ) -> Result<SearchPaths, SearchPathValidationError> {
        fn canonicalize(path: &SystemPath, system: &dyn System) -> SystemPathBuf {
            system
                .canonicalize_path(path)
                .unwrap_or_else(|_| path.to_path_buf())
        }

        let SearchPathSettings {
            extra_paths,
            src_roots,
            custom_typeshed: typeshed,
            site_packages_paths,
            real_stdlib_path,
            misconfiguration_mode,
        } = self;

        let mut static_paths = vec![];

        for path in extra_paths {
            let path = canonicalize(path, system);
            tracing::debug!("Adding extra search-path `{path}`");

            match SearchPath::extra(system, path) {
                Ok(path) => static_paths.push(path),
                Err(err) => {
                    if *misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping invalid extra search-path: {err}");
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        for src_root in src_roots {
            tracing::debug!("Adding first-party search path `{src_root}`");
            match SearchPath::first_party(system, src_root.to_path_buf()) {
                Ok(path) => static_paths.push(path),
                Err(err) => {
                    if *misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping invalid first-party search-path: {err}");
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        let (typeshed_versions, stdlib_path) = if let Some(typeshed) = typeshed {
            let typeshed = canonicalize(typeshed, system);
            tracing::debug!("Adding custom-stdlib search path `{typeshed}`");

            let versions_path = typeshed.join("stdlib/VERSIONS");

            let results = system
                .read_to_string(&versions_path)
                .map_err(
                    |error| SearchPathValidationError::FailedToReadVersionsFile {
                        path: versions_path,
                        error,
                    },
                )
                .and_then(|versions_content| Ok(versions_content.parse()?))
                .and_then(|parsed| Ok((parsed, SearchPath::custom_stdlib(system, &typeshed)?)));

            match results {
                Ok(results) => results,
                Err(err) => {
                    if self.misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping custom-stdlib search-path: {err}");
                        (
                            vendored_typeshed_versions(vendored),
                            SearchPath::vendored_stdlib(),
                        )
                    } else {
                        return Err(err);
                    }
                }
            }
        } else {
            tracing::debug!("Using vendored stdlib");
            (
                vendored_typeshed_versions(vendored),
                SearchPath::vendored_stdlib(),
            )
        };

        let real_stdlib_path = if let Some(path) = real_stdlib_path {
            match SearchPath::real_stdlib(system, path.clone()) {
                Ok(path) => Some(path),
                Err(err) => {
                    if *misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping invalid real-stdlib search-path: {err}");
                        None
                    } else {
                        return Err(err);
                    }
                }
            }
        } else {
            None
        };

        let mut site_packages: Vec<_> = Vec::with_capacity(site_packages_paths.len());

        for path in site_packages_paths {
            tracing::debug!("Adding site-packages search path `{path}`");
            match SearchPath::site_packages(system, path.clone()) {
                Ok(path) => site_packages.push(path),
                Err(err) => {
                    if self.misconfiguration_mode == MisconfigurationMode::UseDefault {
                        tracing::debug!("Skipping invalid site-packages search-path: {err}");
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        // Filter out module resolution paths that point to the same directory
        // on disk (the same invariant maintained by [`sys.path` at runtime]).
        // (Paths may, however, *overlap* -- e.g. you could have both `src/`
        // and `src/foo` as module resolution paths simultaneously.)
        //
        // This code doesn't use an `IndexSet` because the key is the system
        // path and not the search root.
        //
        // [`sys.path` at runtime]: https://docs.python.org/3/library/site.html#module-site
        let mut seen_paths = FxHashSet::with_capacity_and_hasher(static_paths.len(), FxBuildHasher);

        static_paths.retain(|path| {
            if let Some(path) = path.as_system_path() {
                seen_paths.insert(path.to_path_buf())
            } else {
                true
            }
        });

        // Users probably shouldn't do this but... if they've shadowed their stdlib we should deduplicate it away.
        // This notably will mess up anything that checks if a search path "is the standard library" as we won't
        // "remember" that fact for static paths.
        let stdlib_path_is_shadowed = stdlib_path
            .as_system_path()
            .map(|path| seen_paths.contains(path))
            .unwrap_or(false);
        let real_stdlib_path_is_shadowed = real_stdlib_path
            .as_ref()
            .and_then(SearchPath::as_system_path)
            .map(|path| seen_paths.contains(path))
            .unwrap_or(false);

        // Build the search paths using the builder
        let mut builder = SearchPathsBuilder::new(vendored).static_paths(static_paths);

        if !stdlib_path_is_shadowed {
            builder = builder.stdlib_path(stdlib_path, typeshed_versions);
        }

        if !real_stdlib_path_is_shadowed {
            builder = builder.real_stdlib_path(real_stdlib_path);
        }

        builder = builder.site_packages_paths(site_packages);

        Ok(builder.build())
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::Db as _;
    use ruff_db::files::File;
    use ruff_db::system::{DbWithTestSystem as _, DbWithWritableSystem as _, SystemPathBuf};

    use crate::db::tests::TestDb;
    use crate::program::{Program, SearchPathSettings};
    use crate::{ProgramSettings, PythonPlatform, PythonVersionWithSource};
    use ty_module_resolver::{ModuleName, resolve_module_confident};

    #[test]
    fn multiple_site_packages_with_editables() {
        let mut db = TestDb::new();

        let venv_site_packages = SystemPathBuf::from("/venv-site-packages");
        let site_packages_pth = venv_site_packages.join("foo.pth");
        let system_site_packages = SystemPathBuf::from("/system-site-packages");
        let editable_install_location = SystemPathBuf::from("/x/y/a.py");
        let system_site_packages_location = system_site_packages.join("a.py");

        db.memory_file_system()
            .create_directory_all("/src")
            .unwrap();
        db.write_files([
            (&site_packages_pth, "/x/y"),
            (&editable_install_location, ""),
            (&system_site_packages_location, ""),
        ])
        .unwrap();

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersionWithSource::default(),
                python_platform: PythonPlatform::default(),
                search_paths: SearchPathSettings {
                    site_packages_paths: vec![venv_site_packages, system_site_packages],
                    ..SearchPathSettings::new(vec![SystemPathBuf::from("/src")])
                }
                .to_search_paths(db.system(), db.vendored())
                .expect("Valid search path settings"),
            },
        );

        // The editable installs discovered from the `.pth` file in the first `site-packages` directory
        // take precedence over the second `site-packages` directory...
        let a_module_name = ModuleName::new_static("a").unwrap();
        let a_module = resolve_module_confident(&db, &a_module_name).unwrap();
        assert_eq!(
            a_module.file(&db).unwrap().path(&db),
            &editable_install_location
        );

        db.memory_file_system()
            .remove_file(&site_packages_pth)
            .unwrap();
        File::sync_path(&mut db, &site_packages_pth);

        // ...But now that the `.pth` file in the first `site-packages` directory has been deleted,
        // the editable install no longer exists, so the module now resolves to the file in the
        // second `site-packages` directory
        let a_module = resolve_module_confident(&db, &a_module_name).unwrap();
        assert_eq!(
            a_module.file(&db).unwrap().path(&db),
            &system_site_packages_location
        );
    }
}
