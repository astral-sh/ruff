use rustc_hash::{FxBuildHasher, FxHashSet};
use salsa::Durability;

use ruff_db::files::FileRootKind;
use ruff_db::program::{Program, RawProgramSettings, RawSearchPathSettings, SearchPathSettings};

use crate::db::Db;
use crate::path::{SearchPath, SearchPathValidationError};

/// Validate and normalize the raw settings given by the user
/// into settings we can use for module resolution
///
/// This method also implements the typing spec's [module resolution order].
///
/// [module resolution order]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
pub fn try_resolve_module_resolution_settings(
    db: &dyn Db,
    raw_settings: RawSearchPathSettings,
) -> Result<SearchPathSettings, SearchPathValidationError> {
    let RawSearchPathSettings {
        extra_paths,
        src_root,
        custom_typeshed,
        site_packages,
    } = raw_settings;

    let custom_typeshed = custom_typeshed.as_deref();

    if !extra_paths.is_empty() {
        tracing::info!("Extra search paths: {extra_paths:?}");
    }

    if let Some(custom_typeshed) = custom_typeshed {
        tracing::info!("Custom typeshed directory: {custom_typeshed}");
    }

    if !site_packages.is_empty() {
        tracing::info!("Site-packages directories: {site_packages:?}");
    }

    let system = db.system();
    let files = db.files();

    let mut static_search_paths = vec![];

    for path in extra_paths {
        files.try_add_root(db.upcast(), &path, FileRootKind::LibrarySearchPath);
        static_search_paths.push(SearchPath::extra(system, path.clone())?);
    }

    static_search_paths.push(SearchPath::first_party(system, src_root.clone())?);

    static_search_paths.push(if let Some(custom_typeshed) = custom_typeshed {
        files.try_add_root(
            db.upcast(),
            custom_typeshed,
            FileRootKind::LibrarySearchPath,
        );
        SearchPath::custom_stdlib(db, custom_typeshed.to_path_buf())?
    } else {
        SearchPath::vendored_stdlib()
    });

    // TODO vendor typeshed's third-party stubs as well as the stdlib and fallback to them as a final step

    // Filter out module resolution paths that point to the same directory on disk (the same invariant maintained by [`sys.path` at runtime]).
    // (Paths may, however, *overlap* -- e.g. you could have both `src/` and `src/foo`
    // as module resolution paths simultaneously.)
    //
    // [`sys.path` at runtime]: https://docs.python.org/3/library/site.html#module-site
    // This code doesn't use an `IndexSet` because the key is the system path and not the search root.
    let mut seen_paths =
        FxHashSet::with_capacity_and_hasher(static_search_paths.len(), FxBuildHasher);

    static_search_paths.retain(|path| {
        if let Some(path) = path.as_system_path() {
            seen_paths.insert(path.to_path_buf())
        } else {
            true
        }
    });

    Ok(SearchPathSettings {
        static_search_paths: static_search_paths
            .into_iter()
            .map(ruff_db::program::SearchPath::from)
            .collect(),
        site_packages_paths: site_packages.clone(),
    })
}

pub fn program_from_raw_settings(
    db: &dyn Db,
    settings: RawProgramSettings,
) -> Result<Program, SearchPathValidationError> {
    let RawProgramSettings {
        target_version,
        search_paths,
    } = settings;
    try_resolve_module_resolution_settings(db, search_paths).map(|search_path_settings| {
        Program::builder(target_version, search_path_settings)
            .durability(Durability::HIGH)
            .new(db)
    })
}
