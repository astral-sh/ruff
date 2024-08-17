use ruff_db::vendored::VendoredFileSystem;

use super::typeshed::LazyTypeshedVersions;
use crate::db::Db;
use crate::python_version::PythonVersion;

pub(crate) struct ResolverState<'db> {
    pub(crate) db: &'db dyn Db,
    pub(crate) typeshed_versions: LazyTypeshedVersions<'db>,
    pub(crate) target_version: PythonVersion,
}

impl<'db> ResolverState<'db> {
    pub(crate) fn new(db: &'db dyn Db, target_version: PythonVersion) -> Self {
        Self {
            db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version,
        }
    }

    pub(crate) fn vendored(&self) -> &VendoredFileSystem {
        self.db.vendored()
    }
}
